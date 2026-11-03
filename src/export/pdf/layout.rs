//! Laying a note out for print: the Markdown tree becomes a column of
//! positioned items (lines of text, rules, images, table rows) that the
//! paginator cuts into pages. Positions are in points, `x` from the content
//! area's left edge.

use super::fonts::{Face, Fonts, Variant};
use crate::export::html::local_path;
use crate::rich::{Alignment, EditableTable, TableSyntax, html_picture};
use markdown::mdast::{AlignKind, Node};
use std::{collections::HashMap, path::Path};

pub type Rgb = (u8, u8, u8);

pub const TEXT: Rgb = (0x1f, 0x23, 0x28);
pub const MUTED: Rgb = (0x59, 0x63, 0x6e);
pub const LINK: Rgb = (0x09, 0x69, 0xda);
pub const BORDER: Rgb = (0xd1, 0xd9, 0xe0);
pub const SOFT: Rgb = (0xf6, 0xf8, 0xfa);
pub const CODE_INLINE: Rgb = (0xef, 0xf1, 0xf3);
pub const QUOTE_BAR: Rgb = (0xd0, 0xd7, 0xde);

pub const BODY: f32 = 10.5;
const LEADING: f32 = 1.55;
const LIST_INDENT: f32 = 18.;
const QUOTE_INDENT: f32 = 14.;
const CELL_PAD_X: f32 = 7.;
const CELL_PAD_Y: f32 = 5.;
const CODE_PAD: f32 = 9.;

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub variant: Variant,
    pub size: f32,
    pub color: Rgb,
    pub strike: bool,
    pub underline: bool,
    pub link: Option<String>,
    /// A background behind the text, with its opacity.
    pub highlight: Option<(Rgb, f32)>,
    /// Inline code: a soft panel behind it.
    pub code: bool,
    /// Raised above the baseline (superscripts), in points.
    pub raise: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            variant: Variant::default(),
            size: BODY,
            color: TEXT,
            strike: false,
            underline: false,
            link: None,
            highlight: None,
            code: false,
            raise: 0.,
        }
    }
}

/// Text placed on a line.
pub struct Frag {
    pub x: f32,
    pub width: f32,
    pub text: String,
    pub face: &'static Face,
    pub style: Style,
}

pub enum Marker {
    Text { x: f32, text: String, face: &'static Face, size: f32, color: Rgb },
    /// Drawn, not a glyph: fonts disagree on (or lack) bullet characters.
    /// Filled dot, ring, then square, by depth.
    Bullet { x: f32, size: f32, depth: usize, color: Rgb },
    Check { x: f32, size: f32, done: bool },
}

/// A tinted panel behind a run of items (code blocks), rounded at its ends.
#[derive(Clone, Copy)]
pub struct Panel {
    pub x: f32,
    pub width: f32,
    pub color: Rgb,
    pub round_top: bool,
    pub round_bottom: bool,
}

pub struct Line {
    pub height: f32,
    pub baseline: f32,
    pub frags: Vec<Frag>,
    pub marker: Option<Marker>,
    /// Quote bars to the left, at these x positions.
    pub bars: Vec<f32>,
    pub panel: Option<Panel>,
    /// Never the last line on a page (headings).
    pub keep_with_next: bool,
    /// A heading starts here: its level and text, for the PDF's bookmarks.
    pub heading: Option<(u8, String)>,
}

pub struct Cell {
    pub x: f32,
    pub lines: Vec<Line>,
}

pub struct Row {
    pub height: f32,
    pub x: f32,
    pub width: f32,
    pub cells: Vec<Cell>,
    pub header: bool,
}

pub enum Item {
    Line(Line),
    Space { height: f32, bars: Vec<f32>, panel: Option<Panel> },
    Rule { x: f32, width: f32 },
    Image { x: f32, width: f32, height: f32, image: krilla::image::Image },
    Row(Row),
}

impl Item {
    pub fn height(&self) -> f32 {
        match self {
            Item::Line(line) => line.height,
            Item::Space { height, .. } => *height,
            Item::Rule { .. } => 16.,
            Item::Image { height, .. } => *height,
            Item::Row(row) => row.height,
        }
    }
}

/// Inline content before it's wrapped into lines.
enum Inline {
    Text(String, Style),
    Break,
    Image { src: String, alt: String, width: String, alignment: Alignment },
}

/// Where blocks go: their left edge and width, and the quote bars and text
/// color they inherit.
#[derive(Clone)]
struct Frame {
    x: f32,
    width: f32,
    bars: Vec<f32>,
    color: Rgb,
}

pub struct Layout<'a> {
    fonts: &'static Fonts,
    base: Option<&'a Path>,
    images: bool,
    /// The tallest an image may be: one page's content.
    max_image_height: f32,
    pub items: Vec<Item>,
    definitions: HashMap<String, String>,
    footnotes: HashMap<String, Vec<Node>>,
    /// Footnotes in the order they're first referred to.
    footnote_order: Vec<String>,
}

impl<'a> Layout<'a> {
    pub fn new(fonts: &'static Fonts, base: Option<&'a Path>, images: bool, max_image_height: f32) -> Self {
        Self {
            fonts,
            base,
            images,
            max_image_height,
            items: Vec::new(),
            definitions: HashMap::new(),
            footnotes: HashMap::new(),
            footnote_order: Vec::new(),
        }
    }

    /// Lay out the whole note in a column `width` points wide.
    pub fn document(&mut self, root: &Node, width: f32) {
        let children = root.children().map(Vec::as_slice).unwrap_or(&[]);
        self.collect(children);
        let frame = Frame { x: 0., width, bars: Vec::new(), color: TEXT };
        self.blocks(children, &frame);
        self.footnote_section(&frame);
    }

    fn collect(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Definition(d) => {
                    self.definitions.insert(d.identifier.to_lowercase(), d.url.clone());
                }
                Node::FootnoteDefinition(f) => {
                    self.footnotes.insert(f.identifier.to_lowercase(), f.children.clone());
                }
                _ => {
                    if let Some(children) = node.children() {
                        self.collect(children);
                    }
                }
            }
        }
    }

    fn space(&mut self, height: f32, frame: &Frame) {
        self.items.push(Item::Space { height, bars: frame.bars.clone(), panel: None });
    }

    fn blocks(&mut self, nodes: &[Node], frame: &Frame) {
        let mut first = true;
        for node in nodes {
            if matches!(node, Node::Definition(_) | Node::FootnoteDefinition(_) | Node::Yaml(_) | Node::Toml(_)) {
                continue;
            }
            if !first {
                self.space(gap_before(node), frame);
            }
            first = false;
            self.block(node, frame);
        }
    }

    fn block(&mut self, node: &Node, frame: &Frame) {
        let base = Style { color: frame.color, ..Style::default() };
        match node {
            Node::Paragraph(p) => {
                let inlines = self.inlines(&p.children, &base);
                self.paragraph(inlines, frame, None);
            }
            Node::Heading(h) => {
                let level = h.depth.clamp(1, 6);
                let size = [20., 16., 13.5, 12., 11., 10.5][level as usize - 1];
                let style = Style {
                    size,
                    variant: Variant { bold: true, ..Variant::default() },
                    ..base.clone()
                };
                let inlines = self.inlines(&h.children, &style);
                let title = node_text(&h.children);
                let start = self.items.len();
                self.paragraph(inlines, frame, None);
                for (i, item) in self.items[start..].iter_mut().enumerate() {
                    if let Item::Line(line) = item {
                        line.keep_with_next = true;
                        if i == 0 {
                            line.heading = Some((level, title.clone()));
                        }
                    }
                }
            }
            Node::ThematicBreak(_) => self.items.push(Item::Rule { x: frame.x, width: frame.width }),
            Node::Blockquote(q) => {
                let inner = Frame {
                    x: frame.x + QUOTE_INDENT,
                    width: (frame.width - QUOTE_INDENT).max(40.),
                    bars: frame.bars.iter().copied().chain(std::iter::once(frame.x + 1.)).collect(),
                    color: MUTED,
                };
                self.blocks(&q.children, &inner);
            }
            Node::Code(code) if crate::diagram::is_mermaid(code.lang.as_deref()) => {
                if !self.diagram(&code.value, frame) {
                    self.code_block(&code.value, frame);
                }
            }
            Node::Code(code) => self.code_block(&code.value, frame),
            Node::Math(math) => self.code_block(&math.value, frame),
            Node::List(list) => self.list(list, frame, 0),
            Node::Table(table) => {
                let rows: Vec<Vec<Vec<Inline>>> = table
                    .children
                    .iter()
                    .enumerate()
                    .filter_map(|(r, row)| match row {
                        Node::TableRow(row) => Some(
                            row.children
                                .iter()
                                .map(|cell| {
                                    let style = Style {
                                        variant: Variant { bold: r == 0, ..Variant::default() },
                                        ..base.clone()
                                    };
                                    self.inlines(cell.children().map_or(&[][..], |c| c.as_slice()), &style)
                                })
                                .collect(),
                        ),
                        _ => None,
                    })
                    .collect();
                let aligns: Vec<AlignKind> = table.align.clone();
                self.table(rows, &aligns, true, frame);
            }
            Node::Html(html) => self.html_block(&html.value, frame),
            other => {
                if let Some(children) = other.children() {
                    let inlines = self.inlines(children, &base);
                    self.paragraph(inlines, frame, None);
                }
            }
        }
    }

    fn list(&mut self, list: &markdown::mdast::List, frame: &Frame, depth: usize) {
        let inner = Frame {
            x: frame.x + LIST_INDENT,
            width: (frame.width - LIST_INDENT).max(40.),
            ..frame.clone()
        };
        let face = self.fonts.face(Variant::default());
        for (i, item) in list.children.iter().enumerate() {
            let Node::ListItem(item) = item else { continue };
            if i > 0 {
                self.space(if list.spread { 6. } else { 2.5 }, frame);
            }
            let start = self.items.len();
            for (j, child) in item.children.iter().enumerate() {
                if j > 0 {
                    self.space(if list.spread { 6. } else { 2.5 }, &inner);
                }
                match child {
                    Node::List(nested) => self.list(nested, &inner, depth + 1),
                    other => self.block(other, &inner),
                }
            }
            if item.children.is_empty() {
                self.paragraph(vec![Inline::Text(String::new(), Style::default())], &inner, None);
            }
            let marker = match item.checked {
                Some(done) => Marker::Check { x: frame.x + 1., size: BODY * 0.95, done },
                None if list.ordered => {
                    let text = format!("{}.", list.start.unwrap_or(1) as usize + i);
                    let width = face.width(&text, BODY);
                    Marker::Text { x: frame.x + LIST_INDENT - 5. - width, text, face, size: BODY, color: frame.color }
                }
                None => Marker::Bullet { x: frame.x + 5., size: BODY, depth: depth % 3, color: frame.color },
            };
            if let Some(Item::Line(line)) = self.items[start..].iter_mut().find(|i| matches!(i, Item::Line(_))) {
                line.marker = Some(marker);
            }
        }
    }

    fn code_block(&mut self, code: &str, frame: &Frame) {
        let face = self.fonts.face(Variant { mono: true, ..Variant::default() });
        let size = 9.;
        let panel = Panel { x: frame.x, width: frame.width, color: SOFT, round_top: true, round_bottom: false };
        let inner = frame.width - 2. * CODE_PAD;
        let height = size * 1.5;
        let baseline = (height - (face.ascent + face.descent) * size) / 2. + face.ascent * size;
        self.items.push(Item::Space { height: CODE_PAD, bars: frame.bars.clone(), panel: Some(panel) });
        let style = Style { variant: Variant { mono: true, ..Variant::default() }, size, color: TEXT, ..Style::default() };
        let code = code.strip_suffix('\n').unwrap_or(code);
        for source in code.split('\n') {
            let source = source.replace('\t', "    ");
            // Long lines wrap anywhere: code has no words to keep whole.
            let mut rest = source.as_str();
            loop {
                let (piece, tail) = split_to_width(self.fonts, face, rest, size, inner);
                let frags = self.frags(piece, &style, frame.x + CODE_PAD);
                self.items.push(Item::Line(Line {
                    height,
                    baseline,
                    frags,
                    marker: None,
                    bars: frame.bars.clone(),
                    panel: Some(Panel { round_top: false, ..panel }),
                    keep_with_next: false,
                    heading: None,
                }));
                if tail.is_empty() {
                    break;
                }
                rest = tail;
            }
        }
        self.items.push(Item::Space {
            height: CODE_PAD,
            bars: frame.bars.clone(),
            panel: Some(Panel { round_top: false, round_bottom: true, ..panel }),
        });
    }

    fn table(&mut self, rows: Vec<Vec<Vec<Inline>>>, aligns: &[AlignKind], header: bool, frame: &Frame) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        // Each column as wide as its longest cell on one line, or as narrow
        // as its longest word; shared out to fit the frame.
        let mut natural = vec![0f32; columns];
        let mut minimum = vec![0f32; columns];
        for row in &rows {
            for (c, cell) in row.iter().enumerate() {
                let (line, word) = self.measure(cell);
                natural[c] = natural[c].max(line + 2. * CELL_PAD_X);
                minimum[c] = minimum[c].max((word + 2. * CELL_PAD_X).min(frame.width / columns as f32));
            }
        }
        let total: f32 = natural.iter().sum();
        let widths: Vec<f32> = if total <= frame.width {
            natural
        } else {
            let min_total: f32 = minimum.iter().sum();
            let spare = (frame.width - min_total).max(0.);
            let flexible: f32 = natural.iter().zip(&minimum).map(|(n, m)| n - m).sum::<f32>().max(1.);
            natural
                .iter()
                .zip(&minimum)
                .map(|(n, m)| m + spare * (n - m) / flexible)
                .collect()
        };
        let table_width: f32 = widths.iter().sum();
        for (r, row) in rows.into_iter().enumerate() {
            let mut x = frame.x;
            let mut cells = Vec::new();
            let mut height = 0f32;
            for (c, width) in widths.iter().enumerate() {
                let inlines = row.get(c).map(|cell| clone_inlines(cell)).unwrap_or_default();
                let inner = Frame { x: x + CELL_PAD_X, width: width - 2. * CELL_PAD_X, bars: Vec::new(), color: frame.color };
                let start = self.items.len();
                self.paragraph(inlines, &inner, Some(aligns.get(c).copied().unwrap_or(AlignKind::None)));
                let lines: Vec<Line> = self
                    .items
                    .drain(start..)
                    .filter_map(|item| match item {
                        Item::Line(line) => Some(line),
                        _ => None,
                    })
                    .collect();
                height = height.max(lines.iter().map(|l| l.height).sum::<f32>());
                cells.push(Cell { x, lines });
                x += width;
            }
            self.items.push(Item::Row(Row {
                height: height + 2. * CELL_PAD_Y,
                x: frame.x,
                width: table_width,
                cells,
                header: header && r == 0,
            }));
        }
    }

    /// The width of inline content on one line, and of its widest word.
    fn measure(&self, inlines: &[Inline]) -> (f32, f32) {
        let mut line = 0f32;
        let mut word = 0f32;
        for inline in inlines {
            if let Inline::Text(text, style) = inline {
                let face = self.fonts.face(style.variant);
                line += self.fonts.width(text, face, style.size);
                for w in text.split_whitespace() {
                    word = word.max(self.fonts.width(w, face, style.size));
                }
            }
        }
        (line, word)
    }

    fn html_block(&mut self, html: &str, frame: &Frame) {
        if let Some(picture) = html_picture(html.trim()) {
            self.image(&picture.src, &picture.alt, &picture.width, picture.alignment, frame);
            return;
        }
        if let Some(table) = EditableTable::parse_html(html) {
            let header = matches!(table.syntax, TableSyntax::Html { header: true });
            let rows = table
                .rows
                .iter()
                .enumerate()
                .map(|(r, row)| {
                    row.iter()
                        .map(|cell| {
                            let style = Style {
                                variant: Variant { bold: header && r == 0, ..Variant::default() },
                                color: frame.color,
                                ..Style::default()
                            };
                            let mut out = Vec::new();
                            self.html_inline(cell, &mut vec![("".into(), style)], &mut out);
                            out
                        })
                        .collect()
                })
                .collect();
            self.table(rows, &[], header, frame);
            return;
        }
        if self.html_flow(html, frame) {
            return;
        }
        let trimmed = html.trim_start().to_ascii_lowercase();
        let heading = (1..=6).find(|n| trimmed.starts_with(&format!("<h{n}")));
        let style = match heading {
            Some(n) => Style {
                size: [20., 16., 13.5, 12., 11., 10.5][n - 1],
                variant: Variant { bold: true, ..Variant::default() },
                color: frame.color,
                ..Style::default()
            },
            None => Style { color: frame.color, ..Style::default() },
        };
        let mut out = Vec::new();
        self.html_inline(html, &mut vec![("".into(), style)], &mut out);
        // Block tags around the text leave nothing to show.
        if out.iter().any(|i| !matches!(i, Inline::Text(t, _) if t.trim().is_empty())) {
            self.paragraph(out, frame, None);
        }
    }

    /// HTML with block structure: quotes, lists and paragraphs laid out as
    /// their Markdown counterparts are. Returns false when there's none.
    fn html_flow(&mut self, html: &str, frame: &Frame) -> bool {
        let Some((start, name, inner, end)) = find_block(html) else {
            return false;
        };
        let before = &html[..start];
        if !before.trim().is_empty() {
            self.html_inline_paragraph(before, frame);
        }
        let body = &html[inner.clone()];
        match name.as_str() {
            "blockquote" => {
                let quote = Frame {
                    x: frame.x + QUOTE_INDENT,
                    width: (frame.width - QUOTE_INDENT).max(40.),
                    bars: frame.bars.iter().copied().chain(std::iter::once(frame.x + 1.)).collect(),
                    color: MUTED,
                };
                if !self.html_flow(body, &quote) {
                    self.html_inline_paragraph(body, &quote);
                }
            }
            "ul" | "ol" => {
                let inner_frame = Frame {
                    x: frame.x + LIST_INDENT,
                    width: (frame.width - LIST_INDENT).max(40.),
                    ..frame.clone()
                };
                let face = self.fonts.face(Variant::default());
                let mut rest = body;
                let mut n = 0;
                while let Some((s, item, inner, e)) = find_block(rest) {
                    if item != "li" {
                        break;
                    }
                    n += 1;
                    if n > 1 {
                        self.space(2.5, frame);
                    }
                    let at = self.items.len();
                    let content = &rest[inner];
                    if !self.html_flow(content, &inner_frame) {
                        self.html_inline_paragraph(content, &inner_frame);
                    }
                    let marker = if name == "ol" {
                        let text = format!("{n}.");
                        let width = face.width(&text, BODY);
                        Marker::Text { x: frame.x + LIST_INDENT - 5. - width, text, face, size: BODY, color: frame.color }
                    } else {
                        Marker::Bullet { x: frame.x + 5., size: BODY, depth: 0, color: frame.color }
                    };
                    if let Some(Item::Line(line)) = self.items[at..].iter_mut().find(|i| matches!(i, Item::Line(_))) {
                        line.marker = Some(marker);
                    }
                    let _ = s;
                    rest = &rest[e..];
                }
            }
            _ => {
                if !self.html_flow(body, frame) {
                    let tag_end = html[start..].find('>').map_or(start, |i| start + i + 1);
                    self.html_block(&format!("{}{}</{name}>", &html[start..tag_end], body), frame);
                }
            }
        }
        let after = &html[end..];
        if !after.trim().is_empty() {
            self.space(9., frame);
            if !self.html_flow(after, frame) {
                self.html_inline_paragraph(after, frame);
            }
        }
        true
    }

    fn html_inline_paragraph(&mut self, html: &str, frame: &Frame) {
        let style = Style { color: frame.color, ..Style::default() };
        let mut out = Vec::new();
        self.html_inline(html, &mut vec![(String::new(), style)], &mut out);
        if out.iter().any(|i| !matches!(i, Inline::Text(t, _) if t.trim().is_empty())) {
            self.paragraph(out, frame, None);
        }
    }

    fn inlines(&mut self, nodes: &[Node], style: &Style) -> Vec<Inline> {
        let mut out = Vec::new();
        let mut html = vec![(String::new(), style.clone())];
        for node in nodes {
            self.inline(node, &mut html, &mut out);
        }
        out
    }

    /// One inline node. `stack` holds the styles of HTML tags still open in
    /// this paragraph; its top is the style text takes.
    fn inline(&mut self, node: &Node, stack: &mut Vec<(String, Style)>, out: &mut Vec<Inline>) {
        let style = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
        let nested = |this: &mut Self, children: &[Node], style: Style, out: &mut Vec<Inline>| {
            let mut inner = vec![(String::new(), style)];
            for child in children {
                this.inline(child, &mut inner, out);
            }
        };
        match node {
            Node::Text(t) => out.push(Inline::Text(t.value.replace('\n', " "), style)),
            Node::Strong(n) => {
                let s = Style { variant: Variant { bold: true, ..style.variant }, ..style };
                nested(self, &n.children, s, out);
            }
            Node::Emphasis(n) => {
                let s = Style { variant: Variant { italic: true, ..style.variant }, ..style };
                nested(self, &n.children, s, out);
            }
            Node::Delete(n) => nested(self, &n.children, Style { strike: true, ..style }, out),
            Node::InlineCode(code) => out.push(Inline::Text(
                code.value.clone(),
                Style {
                    variant: Variant { mono: true, ..Variant::default() },
                    size: style.size * 0.9,
                    code: true,
                    ..style
                },
            )),
            Node::InlineMath(math) => out.push(Inline::Text(format!("${}$", math.value), style)),
            Node::Break(_) => out.push(Inline::Break),
            Node::Link(link) => {
                let s = link_style(&style, &link.url);
                nested(self, &link.children, s, out);
            }
            Node::LinkReference(link) => {
                let s = match self.definitions.get(&link.identifier.to_lowercase()) {
                    Some(url) => link_style(&style, url),
                    None => style,
                };
                nested(self, &link.children, s, out);
            }
            Node::Image(image) => out.push(Inline::Image {
                src: image.url.clone(),
                alt: image.alt.clone(),
                width: String::new(),
                alignment: Alignment::Left,
            }),
            Node::ImageReference(image) => {
                if let Some(url) = self.definitions.get(&image.identifier.to_lowercase()) {
                    out.push(Inline::Image {
                        src: url.clone(),
                        alt: image.alt.clone(),
                        width: String::new(),
                        alignment: Alignment::Left,
                    });
                }
            }
            Node::FootnoteReference(note) => {
                let id = note.identifier.to_lowercase();
                let number = match self.footnote_order.iter().position(|n| *n == id) {
                    Some(at) => at + 1,
                    None => {
                        self.footnote_order.push(id);
                        self.footnote_order.len()
                    }
                };
                out.push(Inline::Text(
                    number.to_string(),
                    Style { size: style.size * 0.7, raise: style.size * 0.38, color: LINK, ..style },
                ));
            }
            Node::Html(html) => self.html_inline(&html.value, stack, out),
            other => {
                if let Some(children) = other.children() {
                    for child in children {
                        self.inline(child, stack, out);
                    }
                }
            }
        }
    }

    /// Inline HTML: tags change the style of the text that follows until
    /// they close; `<br>` and `<img>` become a break and an image.
    fn html_inline(&mut self, html: &str, stack: &mut Vec<(String, Style)>, out: &mut Vec<Inline>) {
        let mut rest = html;
        while !rest.is_empty() {
            let style = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
            let Some(start) = rest.find('<') else {
                out.push(Inline::Text(entities(rest), style));
                break;
            };
            if start > 0 {
                out.push(Inline::Text(entities(&rest[..start]), style.clone()));
            }
            let Some(len) = rest[start..].find('>') else {
                out.push(Inline::Text(entities(&rest[start..]), style));
                break;
            };
            let tag = &rest[start..start + len + 1];
            rest = &rest[start + len + 1..];
            let inner = tag.trim_start_matches('<').trim_end_matches('>').trim_end_matches('/').trim();
            if let Some(name) = inner.strip_prefix('/') {
                let name = name.trim().to_ascii_lowercase();
                if let Some(at) = stack.iter().rposition(|(n, _)| *n == name) {
                    stack.truncate(at.max(1));
                }
                continue;
            }
            let name: String = inner.split(char::is_whitespace).next().unwrap_or_default().to_ascii_lowercase();
            let opened = match name.as_str() {
                "b" | "strong" => Some(Style { variant: Variant { bold: true, ..style.variant }, ..style.clone() }),
                "i" | "em" => Some(Style { variant: Variant { italic: true, ..style.variant }, ..style.clone() }),
                "u" | "ins" => Some(Style { underline: true, ..style.clone() }),
                "s" | "del" | "strike" => Some(Style { strike: true, ..style.clone() }),
                "mark" => Some(Style { highlight: Some(mark_color(tag)), ..style.clone() }),
                "code" => Some(Style {
                    variant: Variant { mono: true, ..Variant::default() },
                    size: style.size * 0.9,
                    code: true,
                    ..style.clone()
                }),
                "sub" => Some(Style { size: style.size * 0.75, raise: -style.size * 0.15, ..style.clone() }),
                "sup" => Some(Style { size: style.size * 0.75, raise: style.size * 0.35, ..style.clone() }),
                "a" => attribute(tag, "href").map(|url| link_style(&style, &url)),
                "br" => {
                    out.push(Inline::Break);
                    None
                }
                "img" => {
                    if let Some(picture) = html_picture(tag) {
                        out.push(Inline::Image {
                            src: picture.src,
                            alt: picture.alt,
                            width: picture.width,
                            alignment: picture.alignment,
                        });
                    }
                    None
                }
                _ => None,
            };
            if let Some(opened) = opened {
                if !tag.trim_end_matches('>').ends_with('/') {
                    stack.push((name, opened));
                }
            }
        }
    }

    /// Wrap inline content into lines across the frame; images in it break
    /// the paragraph and stand on their own.
    fn paragraph(&mut self, inlines: Vec<Inline>, frame: &Frame, align: Option<AlignKind>) {
        let mut run = Vec::new();
        for inline in inlines {
            match inline {
                Inline::Image { src, alt, width, alignment } => {
                    if !run.is_empty() {
                        self.wrap(std::mem::take(&mut run), frame, align);
                    }
                    self.image(&src, &alt, &width, alignment, frame);
                }
                other => run.push(other),
            }
        }
        if !run.is_empty() || self.items.is_empty() {
            self.wrap(run, frame, align);
        }
    }

    fn wrap(&mut self, inlines: Vec<Inline>, frame: &Frame, align: Option<AlignKind>) {
        // Pieces: the text between break opportunities, each in one style.
        struct Piece {
            text: String,
            style: Style,
            width: f32,
            /// Trailing spaces, which don't count at the end of a line.
            trailing: f32,
            /// A line may start here.
            can_break: bool,
            hard_break: bool,
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for inline in &inlines {
            match inline {
                Inline::Break => pieces.push(Piece {
                    text: String::new(),
                    style: Style::default(),
                    width: 0.,
                    trailing: 0.,
                    can_break: true,
                    hard_break: true,
                }),
                Inline::Text(text, style) => {
                    let face = self.fonts.face(style.variant);
                    let mut last = 0;
                    let breaks: Vec<usize> = unicode_linebreak::linebreaks(text).map(|(at, _)| at).collect();
                    for (i, at) in breaks.into_iter().enumerate() {
                        let piece = &text[last..at];
                        last = at;
                        if piece.is_empty() {
                            continue;
                        }
                        let body = piece.trim_end();
                        let width = self.fonts.width(body, face, style.size);
                        let full = if body.len() == piece.len() {
                            width
                        } else {
                            self.fonts.width(piece, face, style.size)
                        };
                        pieces.push(Piece {
                            text: piece.to_string(),
                            style: style.clone(),
                            width: full,
                            trailing: full - width,
                            // Within a span, every piece starts at a break
                            // opportunity; at a span's start only if the text
                            // before ended with a space.
                            can_break: i > 0
                                || pieces.last().is_none_or(|p| p.text.ends_with(char::is_whitespace) || p.hard_break),
                            hard_break: false,
                        });
                    }
                }
                Inline::Image { .. } => {}
            }
        }
        // Greedy filling.
        let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
        let mut used = 0f32;
        for piece in pieces {
            if piece.hard_break {
                lines.push(Vec::new());
                used = 0.;
                continue;
            }
            let fits = used + piece.width - piece.trailing <= frame.width + 0.01;
            if !fits && piece.can_break && !lines.last().unwrap().is_empty() {
                lines.push(Vec::new());
                used = 0.;
            }
            if piece.width - piece.trailing > frame.width {
                // A word wider than the line (a long URL): split it.
                let face = self.fonts.face(piece.style.variant);
                let mut rest = piece.text.as_str();
                while !rest.is_empty() {
                    let room = (frame.width - used).max(frame.width * 0.25);
                    let (head, tail) = split_to_width(self.fonts, face, rest, piece.style.size, room);
                    let width = self.fonts.width(head, face, piece.style.size);
                    lines.last_mut().unwrap().push(Piece {
                        text: head.to_string(),
                        style: piece.style.clone(),
                        width,
                        trailing: 0.,
                        can_break: true,
                        hard_break: false,
                    });
                    used += width;
                    rest = tail;
                    if !rest.is_empty() {
                        lines.push(Vec::new());
                        used = 0.;
                    }
                }
                continue;
            }
            used += piece.width;
            lines.last_mut().unwrap().push(piece);
        }
        let default_size = inlines
            .iter()
            .find_map(|i| match i {
                Inline::Text(_, s) => Some(s.size),
                _ => None,
            })
            .unwrap_or(BODY);
        for pieces in lines {
            let mut frags = Vec::new();
            let mut x = frame.x;
            for piece in &pieces {
                let placed = self.frags(&piece.text, &piece.style, x);
                x += placed.iter().map(|f| f.width).sum::<f32>();
                frags.extend(placed);
            }
            // Spaces at the line's end don't take room (or underline).
            if let Some(last) = frags.last_mut() {
                let trimmed = last.text.trim_end().to_string();
                if trimmed.len() != last.text.len() {
                    last.width = self.fonts.width(&trimmed, last.face, last.style.size);
                    last.text = trimmed;
                }
            }
            let used: f32 = frags.iter().map(|f| f.width).sum();
            let shift = match align {
                Some(AlignKind::Center) => (frame.width - used) / 2.,
                Some(AlignKind::Right) => frame.width - used,
                _ => 0.,
            }
            .max(0.);
            for frag in &mut frags {
                frag.x += shift;
            }
            let size = frags.iter().map(|f| f.style.size).fold(default_size, f32::max);
            let ascent = frags.iter().map(|f| f.face.ascent * f.style.size).fold(0., f32::max);
            let descent = frags.iter().map(|f| f.face.descent * f.style.size).fold(0., f32::max);
            let (ascent, descent) = if frags.is_empty() {
                let face = self.fonts.face(Variant::default());
                (face.ascent * size, face.descent * size)
            } else {
                (ascent, descent)
            };
            let height = (size * LEADING).max(ascent + descent);
            self.items.push(Item::Line(Line {
                height,
                baseline: (height - (ascent + descent)) / 2. + ascent,
                frags,
                marker: None,
                bars: frame.bars.clone(),
                panel: None,
                keep_with_next: false,
                heading: None,
            }));
        }
    }

    /// Text in one style as fragments, one per font it needs.
    fn frags(&self, text: &str, style: &Style, x: f32) -> Vec<Frag> {
        let primary = self.fonts.face(style.variant);
        let mut x = x;
        self.fonts
            .runs(text, primary)
            .into_iter()
            .map(|(face, range)| {
                let text = text[range].to_string();
                let width = face.width(&text, style.size);
                let frag = Frag { x, width, text, face, style: style.clone() };
                x += width;
                frag
            })
            .collect()
    }

    fn image(&mut self, src: &str, alt: &str, width: &str, alignment: Alignment, frame: &Frame) {
        if !self.images {
            return;
        }
        let loaded = local_path(src, self.base)
            .filter(|p| p.is_file())
            .and_then(|path| load_image(&path));
        let Some(image) = loaded else {
            let label = if alt.is_empty() { src } else { alt };
            let style = Style {
                variant: Variant { italic: true, ..Variant::default() },
                color: MUTED,
                ..Style::default()
            };
            self.wrap(vec![Inline::Text(format!("[Image: {label}]"), style)], frame, None);
            return;
        };
        let (px_w, px_h) = image.size();
        let natural = px_w as f32 * 0.75;
        let wanted = width.trim();
        let mut w = if let Some(percent) = wanted.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()) {
            frame.width * percent.clamp(1., 100.) / 100.
        } else if let Ok(px) = wanted.trim_end_matches("px").parse::<f32>() {
            px * 0.75
        } else {
            natural
        };
        w = w.min(frame.width);
        let mut h = w * px_h as f32 / px_w.max(1) as f32;
        if h > self.max_image_height {
            w *= self.max_image_height / h;
            h = self.max_image_height;
        }
        let x = match alignment {
            Alignment::Left => frame.x,
            Alignment::Center => frame.x + (frame.width - w) / 2.,
            Alignment::Right => frame.x + frame.width - w,
        };
        self.items.push(Item::Image { x, width: w, height: h, image });
    }

    /// A Mermaid diagram as a picture three times its size (sharp in print),
    /// centred and no wider than the column. False if it couldn't be drawn.
    fn diagram(&mut self, source: &str, frame: &Frame) -> bool {
        let Ok(drawn) = crate::diagram::render(source, false) else {
            return false;
        };
        let Some(image) = crate::diagram::png(&drawn, 3.)
            .and_then(|png| krilla::image::Image::from_png(krilla::Data::from(png), true).ok())
        else {
            return false;
        };
        // CSS pixels to points.
        let mut w = (drawn.width * 0.75).min(frame.width);
        let mut h = w * drawn.height / drawn.width.max(1.);
        if h > self.max_image_height {
            w *= self.max_image_height / h;
            h = self.max_image_height;
        }
        self.items.push(Item::Image { x: frame.x + (frame.width - w) / 2., width: w, height: h, image });
        true
    }

    /// Footnotes, numbered as referred to, after a short rule at the end.
    fn footnote_section(&mut self, frame: &Frame) {
        if self.footnote_order.is_empty() {
            return;
        }
        self.space(18., frame);
        self.items.push(Item::Rule { x: frame.x, width: frame.width * 0.3 });
        let order = self.footnote_order.clone();
        let small = Style { size: 9., color: MUTED, ..Style::default() };
        for (n, id) in order.iter().enumerate() {
            let children = self.footnotes.get(id).cloned().unwrap_or_default();
            let mut inlines = vec![Inline::Text(format!("{}. ", n + 1), small.clone())];
            for child in &children {
                let mut stack = vec![(String::new(), small.clone())];
                if let Some(grand) = child.children() {
                    for node in grand {
                        self.inline(node, &mut stack, &mut inlines);
                    }
                }
            }
            self.paragraph(inlines, frame, None);
        }
    }
}

/// The first block-level tag in `html` that has more structure than text:
/// its start, name, the range of its content and its end.
fn find_block(html: &str) -> Option<(usize, String, std::ops::Range<usize>, usize)> {
    const BLOCKS: &[&str] = &["blockquote", "ul", "ol", "li", "p", "div"];
    let lower = html.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find('<').map(|i| i + at) {
        let rest = &lower[i + 1..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if BLOCKS.contains(&name.as_str()) {
            let open_end = lower[i..].find('>')? + i + 1;
            // The matching close, counting nested tags of the same name.
            let (open, close) = (format!("<{name}"), format!("</{name}>"));
            let mut depth = 1;
            let mut scan = open_end;
            loop {
                let next_open = lower[scan..].find(&open).map(|o| o + scan);
                let next_close = lower[scan..].find(&close).map(|c| c + scan)?;
                match next_open {
                    Some(o) if o < next_close => {
                        depth += 1;
                        scan = o + open.len();
                    }
                    _ => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((i, name, open_end..next_close, next_close + close.len()));
                        }
                        scan = next_close + close.len();
                    }
                }
            }
        }
        at = i + 1;
    }
    None
}

/// Vertical space before a block, by what it is.
fn gap_before(node: &Node) -> f32 {
    match node {
        Node::Heading(h) if h.depth <= 2 => 18.,
        Node::Heading(_) => 14.,
        _ => 9.,
    }
}

fn link_style(style: &Style, url: &str) -> Style {
    // Only links a PDF reader can follow are links; others stay text.
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
        Style { color: LINK, underline: true, link: Some(url.to_string()), ..style.clone() }
    } else {
        style.clone()
    }
}

/// The longest start of `text` that fits `width`, and the rest.
fn split_to_width<'t>(fonts: &Fonts, face: &Face, text: &'t str, size: f32, width: f32) -> (&'t str, &'t str) {
    if fonts.width(text, face, size) <= width {
        return (text, "");
    }
    let mut end = 0;
    for (at, c) in text.char_indices() {
        let next = at + c.len_utf8();
        if fonts.width(&text[..next], face, size) > width && end > 0 {
            break;
        }
        end = next;
    }
    text.split_at(end)
}

fn clone_inlines(inlines: &[Inline]) -> Vec<Inline> {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t, s) => Inline::Text(t.clone(), s.clone()),
            Inline::Break => Inline::Break,
            Inline::Image { src, alt, width, alignment } => Inline::Image {
                src: src.clone(),
                alt: alt.clone(),
                width: width.clone(),
                alignment: *alignment,
            },
        })
        .collect()
}

fn node_text(nodes: &[Node]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            Node::Text(t) => t.value.clone(),
            Node::InlineCode(c) => c.value.clone(),
            other => other.children().map(|c| node_text(c)).unwrap_or_default(),
        })
        .collect()
}

fn load_image(path: &Path) -> Option<krilla::image::Image> {
    let data = std::fs::read(path).ok()?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let data = krilla::Data::from(data);
    match extension.as_str() {
        "png" => krilla::image::Image::from_png(data, true).ok(),
        "jpg" | "jpeg" => krilla::image::Image::from_jpeg(data, true).ok(),
        "gif" => krilla::image::Image::from_gif(data, true).ok(),
        "webp" => krilla::image::Image::from_webp(data, true).ok(),
        _ => None,
    }
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let quote = tag[at..].chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = tag[at + 1..].find(quote)? + at + 1;
        Some(entities(&tag[at + 1..end]))
    } else {
        let end = tag[at..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map_or(tag.len(), |e| e + at);
        Some(tag[at..end].to_string())
    }
}

/// A `<mark>`'s color: its `color` attribute or CSS background, else the
/// reader's yellow.
fn mark_color(tag: &str) -> (Rgb, f32) {
    let value = attribute(tag, "color").or_else(|| {
        attribute(tag, "style")?.split(';').find_map(|decl| {
            let (key, value) = decl.split_once(':')?;
            key.trim().eq_ignore_ascii_case("background-color").then(|| value.trim().to_string())
        })
    });
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some(v) if v.starts_with('#') => {
            let v = &v[1..];
            let expand = |c: &str| c.repeat(2);
            let full = match v.len() {
                3 | 4 => v.chars().map(|c| expand(&c.to_string())).collect::<String>(),
                _ => v.to_string(),
            };
            match (hex(full.get(0..2).unwrap_or("")), hex(full.get(2..4).unwrap_or("")), hex(full.get(4..6).unwrap_or(""))) {
                (Some(r), Some(g), Some(b)) => {
                    let alpha = full.get(6..8).and_then(hex).map_or(1., |a| a as f32 / 255.);
                    ((r, g, b), alpha)
                }
                _ => ((0xfe, 0xf0, 0x8a), 1.),
            }
        }
        Some("blue") => ((0x3b, 0x82, 0xf6), 1.),
        Some("black") => ((0, 0, 0), 1.),
        Some("white") => ((255, 255, 255), 1.),
        _ => ((0xfe, 0xf0, 0x8a), 1.),
    }
}

fn entities(text: &str) -> String {
    text.replace("&nbsp;", "\u{a0}")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

