use std::{
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    panic::{AssertUnwindSafe, catch_unwind},
};

use gpui_kit::base::text::editor_host::{
    BlockNode, CodeBlock, InlineNode, Paragraph, Table, parse_markdown,
};

use super::document::{Item, Kind, Row, fresh_id, is_closing_fence};
use super::mapping::{LeafBuilder, LeafMap, Seg};

pub(super) fn parse_nodes(text: &str, front_matter: bool) -> Option<Vec<BlockNode>> {
    let extensions = super::math::extensions(front_matter);
    catch_unwind(AssertUnwindSafe(|| parse_markdown(text, extensions).ok()))
        .ok()
        .flatten()
}

pub(super) fn span_of(node: &BlockNode) -> Option<Range<usize>> {
    node.span().map(|s| s.start..s.end)
}

#[derive(Default)]
pub(super) struct Sig(DefaultHasher);

impl Sig {
    pub fn add(&mut self, value: impl Hash) {
        value.hash(&mut self.0);
    }

    pub fn finish(&self) -> u64 {
        self.0.finish()
    }
}

pub(super) fn code_sig(lang: Option<&str>, meta: Option<&str>, code: &str) -> u64 {
    let mut sig = Sig::default();
    sig.add(("code", lang, meta, code));
    sig.finish()
}

pub(super) fn list_sig(ordered: bool, start: Option<u32>, items: &[Item]) -> u64 {
    let mut sig = Sig::default();
    sig.add(("list", ordered, start));
    for item in items {
        sig.add(item.sig);
    }
    sig.finish()
}

pub(super) fn table_sig(aligns: &str, rows: &[Row]) -> u64 {
    let mut sig = Sig::default();
    sig.add(("table", aligns));
    for row in rows {
        sig.add(row.sig);
    }
    sig.finish()
}

pub(super) struct Builder<'a> {
    pub source: &'a str,
    pub block_start: usize,
    pub reuse: &'a mut std::vec::IntoIter<u64>,
    pub leaves: Vec<LeafMap>,
    pub depth: usize,
    pub kind: Kind,
    pub defs: Vec<Range<usize>>,
    pub sig: Sig,
}

impl<'a> Builder<'a> {
    pub fn new(
        source: &'a str,
        block_start: usize,
        depth: usize,
        reuse: &'a mut std::vec::IntoIter<u64>,
    ) -> Self {
        Self {
            source,
            block_start,
            reuse,
            leaves: Vec::new(),
            depth,
            kind: Kind::Other,
            defs: Vec::new(),
            sig: Sig::default(),
        }
    }

    fn key(&mut self) -> u64 {
        self.reuse.next().unwrap_or_else(fresh_id)
    }

    fn rel(&self, offset: usize) -> usize {
        offset.saturating_sub(self.block_start)
    }

    fn nested(&mut self, node: BlockNode) -> BlockNode {
        self.depth += 1;
        let out = self.block(node);
        self.depth -= 1;
        out
    }

    pub fn block(&mut self, node: BlockNode) -> BlockNode {
        let span = span_of(&node);
        match node {
            BlockNode::Paragraph(p)
                if self.depth == 0
                    && let Some(image) = span
                        .as_ref()
                        .and_then(|span| self.source.get(span.clone()))
                        .and_then(|text| super::image::lone_image(&p, text)) =>
            {
                self.sig.add((
                    "img",
                    &image.url,
                    &image.alt,
                    &image.title,
                    image.width,
                    image.align as u8,
                ));
                self.kind = Kind::Image(image);
                BlockNode::Paragraph(p)
            }
            BlockNode::Root { children, span: s }
                if self.depth == 0
                    && let [BlockNode::Paragraph(p)] = children.as_slice()
                    && let Some(image) = span
                        .as_ref()
                        .and_then(|span| self.source.get(span.clone()))
                        .and_then(|text| super::image::lone_image(p, text)) =>
            {
                self.sig.add((
                    "img",
                    &image.url,
                    &image.alt,
                    &image.title,
                    image.width,
                    image.align as u8,
                ));
                self.kind = Kind::Image(image);
                BlockNode::Root { children, span: s }
            }
            BlockNode::Paragraph(p) => {
                self.sig.add("p");
                let empty = span.as_ref().map_or(0, |s| self.rel(s.end));
                BlockNode::Paragraph(self.paragraph(&p, span.clone(), empty))
            }
            BlockNode::Heading {
                level,
                children,
                span: s,
            } => {
                self.sig.add(("h", level));
                let empty = span.as_ref().map_or(0, |s| self.rel(s.end));
                BlockNode::Heading {
                    level,
                    children: self.paragraph(&children, span.clone(), empty),
                    span: s,
                }
            }
            BlockNode::Blockquote { children, span: s } => {
                self.sig.add("quote");
                BlockNode::Blockquote {
                    children: children.into_iter().map(|c| self.nested(c)).collect(),
                    span: s,
                }
            }
            BlockNode::List {
                children,
                ordered,
                start,
                span: s,
            } => {
                let top = self.depth == 0;
                let mut items = Vec::new();
                let mut nodes = Vec::new();
                for child in children {
                    let before = self.leaves.len();
                    let item_span = span_of(&child);
                    let outer = std::mem::take(&mut self.sig);
                    let node = self.nested(child);
                    let item_sig = std::mem::replace(&mut self.sig, outer).finish();
                    self.sig.add(item_sig);
                    if let Some(item_span) = item_span {
                        items.push(Item {
                            span: self.rel(item_span.start)..self.rel(item_span.end),
                            leaves: before..self.leaves.len(),
                            sig: item_sig,
                        });
                    }
                    nodes.push(node);
                }
                if top {
                    self.sig = Sig::default();
                    self.sig.add(list_sig(ordered, start, &items));
                    self.kind = Kind::List { items };
                }
                BlockNode::List {
                    children: nodes,
                    ordered,
                    start,
                    span: s,
                }
            }
            BlockNode::ListItem {
                children,
                spread,
                checked,
                span: s,
            } => {
                self.sig.add(("item", spread, checked));
                let before = self.leaves.len();
                let mut children: Vec<BlockNode> =
                    children.into_iter().map(|c| self.nested(c)).collect();
                if self.leaves.len() == before {
                    let empty = span.as_ref().map_or(0, |s| self.rel(s.end));
                    let key = self.key();
                    self.leaves.push(LeafMap {
                        key,
                        text: String::new(),
                        segs: Vec::new(),
                        empty_src: empty,
                    });
                    children.push(BlockNode::Paragraph(
                        Paragraph::default().with_leaf_key(key),
                    ));
                }
                BlockNode::ListItem {
                    children,
                    spread,
                    checked,
                    span: s,
                }
            }
            BlockNode::CodeBlock(code) => self.code(code, span),
            BlockNode::Table(table) => self.table(table, span),
            BlockNode::Definition {
                identifier,
                url,
                title,
                span: s,
            } => {
                if let Some(span) = &span {
                    self.defs.push(self.rel(span.start)..self.rel(span.end));
                }
                self.sig
                    .add(("def", identifier.as_ref(), url.as_ref(), title.as_deref()));
                BlockNode::Definition {
                    identifier,
                    url,
                    title,
                    span: s,
                }
            }
            BlockNode::HorizontalRule { span: s } => {
                self.sig.add("hr");
                BlockNode::HorizontalRule { span: s }
            }
            other => {
                let source = span
                    .as_ref()
                    .and_then(|s| self.source.get(s.clone()))
                    .unwrap_or("");
                self.sig.add(("atom", source));
                other
            }
        }
    }

    pub fn paragraph(
        &mut self,
        p: &Paragraph,
        span: Option<Range<usize>>,
        empty_src: usize,
    ) -> Paragraph {
        let mut text = String::new();
        let mut segs: Vec<Seg> = Vec::new();
        for node in p.children() {
            self.inline_sig(node);
            let offset = text.len();
            if let Some(image) = node.as_image() {
                text.push('\u{FFFC}');
                if let Some(s) = image.span() {
                    segs.push(Seg {
                        leaf: offset..text.len(),
                        src: self.rel(s.start)..self.rel(s.end),
                        linear: false,
                        line_end: false,
                    });
                }
                continue;
            }
            if node.as_custom().is_some() {
                text.push('\u{FFFC}');
                continue;
            }
            text.push_str(node.text());
            for s in node.source_mapping() {
                let rendered = &node.text()[s.rendered()];
                let source = self.source.get(s.source()).unwrap_or("");
                push_mapped(
                    &mut segs,
                    rendered,
                    source,
                    offset + s.rendered().start,
                    self.rel(s.source().start),
                    s.linear(),
                );
            }
        }
        if segs.is_empty() && !text.is_empty() && span.is_none() {
            return p.clone();
        }
        let bounds = span.map(|s| self.rel(s.start)..self.rel(s.end));
        let segs = fill_gaps(segs, text.len(), bounds.clone(), empty_src);
        let key = self.key();
        self.leaves.push(LeafMap {
            key,
            text,
            segs,
            empty_src,
        });
        p.clone().with_leaf_key(key)
    }

    fn inline_sig(&mut self, node: &InlineNode) {
        self.sig.add(node.text());
        if let Some(image) = node.as_image() {
            self.sig.add((
                image.url.as_ref(),
                format!("{:?}{:?}", image.width, image.height),
            ));
        }
        self.sig.add(format!("{:?}", node.mark_runs()));
    }

    fn code(&mut self, code: CodeBlock, span: Option<Range<usize>>) -> BlockNode {
        let value = code.code().to_string();
        let lang = code.lang().map(|l| l.to_string());
        let meta = code.meta().map(|m| m.to_string());
        self.sig
            .add(code_sig(lang.as_deref(), meta.as_deref(), &value));
        let span = span.unwrap_or(0..0);
        let src = self.source.get(span.clone()).unwrap_or("");
        let first = src.trim_start();
        if self.depth == 0 && first.starts_with("$$") {
            self.kind = Kind::Math { tex: value };
            return BlockNode::CodeBlock(code);
        }
        let fenced = first.starts_with("```")
            || first.starts_with("~~~")
            || first.starts_with("---")
            || first.starts_with("+++")
            || first.starts_with("$$");
        let body = if fenced {
            src.find('\n').map_or(src.len(), |i| i + 1)
        } else {
            0
        };
        let base = self.rel(span.start);
        let mut b = LeafBuilder::default();
        b.push_aligned(&value, &src[body.min(src.len())..], base + body);
        let (text, segs) = (b.text, b.segs);
        let key = self.key();
        self.leaves.push(LeafMap {
            key,
            text,
            segs,
            empty_src: base + body,
        });
        let starts_line = self.source[..span.start.min(self.source.len())]
            .chars()
            .next_back()
            .is_none_or(|c| c == '\n');
        let front_matter = matches!(lang.as_deref(), Some("yml" | "yaml" | "toml"))
            && span.start == 0
            && self.block_start == 0;
        if self.depth == 0
            && starts_line
            && let Some(ch) = src
                .bytes()
                .next()
                .filter(|c| matches!(c, b'`' | b'~') || (front_matter && matches!(c, b'-' | b'+')))
        {
            let len = src.bytes().take_while(|&c| c == ch).count();
            if len >= 3 {
                let trimmed = src.trim_end_matches(['\n', '\r']);
                let last_line_start = trimmed.rfind('\n').map(|i| i + 1);
                let closed = last_line_start
                    .filter(|&s| s >= body)
                    .is_some_and(|s| is_closing_fence(&src[s..], ch, len));
                let body_end = if closed {
                    last_line_start.unwrap()
                } else {
                    src.len()
                };
                self.kind = Kind::Fence {
                    body: body..body_end.max(body),
                    closed,
                    ch,
                    len,
                    lang: lang.clone(),
                    meta: meta.clone(),
                };
            }
        }
        BlockNode::CodeBlock(code.with_leaf_key(key))
    }

    fn table(&mut self, table: Table, span: Option<Range<usize>>) -> BlockNode {
        let aligns = table.column_aligns().to_vec();
        let aligns_key = format!("{aligns:?}");
        let span = span.unwrap_or(0..0);
        let src = self.source.get(span.clone()).unwrap_or("");
        let mut line_spans = Vec::new();
        let mut at = span.start;
        for line in src.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            line_spans.push(at..at + body.len());
            at += line.len();
        }
        let row_lines: Vec<Range<usize>> = line_spans
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, r)| r.clone())
            .collect();
        let mut rows = Vec::new();
        let mut cells_out = Vec::new();
        for (r, row) in table.rows().enumerate() {
            let before = self.leaves.len();
            let line = row_lines.get(r).cloned();
            let cell_spans = line
                .as_ref()
                .map(|l| cell_spans(&self.source[l.clone()], l.start))
                .unwrap_or_default();
            let outer = std::mem::take(&mut self.sig);
            let mut cells = Vec::new();
            for (c, p) in row.enumerate() {
                let cell = cell_spans.get(c).cloned();
                let empty = cell.as_ref().map_or_else(
                    || line.as_ref().map_or(0, |l| self.rel(l.end)),
                    |s| self.rel(s.start),
                );
                self.sig.add(("cell", c));
                cells.push(self.paragraph(p, cell, empty));
            }
            let row_sig = std::mem::replace(&mut self.sig, outer).finish();
            self.sig.add(row_sig);
            if let Some(line) = line {
                rows.push(Row {
                    span: self.rel(line.start)..self.rel(line.end),
                    cells: cells.clone(),
                    leaves: before..self.leaves.len(),
                    sig: row_sig,
                });
            }
            cells_out.push(cells);
        }
        if self.depth == 0 && rows.len() == cells_out.len() {
            self.sig = Sig::default();
            self.sig.add(table_sig(&aligns_key, &rows));
            self.kind = Kind::Table {
                rows,
                aligns: aligns.clone(),
            };
        }
        BlockNode::Table(Table::new(cells_out, aligns))
    }
}

pub(super) fn cell_spans(line: &str, base: usize) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut pipes = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'|' => pipes.push(i),
            _ => {}
        }
        i += 1;
    }
    let lead = line.len() - line.trim_start().len();
    let mut bounds = Vec::new();
    let mut start = lead;
    if pipes.first() == Some(&lead) {
        start = lead + 1;
    }
    let first = start;
    for &p in pipes.iter().filter(|&&p| p >= first) {
        bounds.push(start..p);
        start = p + 1;
    }
    let trail = line.trim_end().len();
    if start < trail {
        bounds.push(start..trail);
    }
    bounds
        .into_iter()
        .map(|r| {
            let cell = &line[r.clone()];
            let s = r.start + (cell.len() - cell.trim_start().len());
            let e = (r.start + cell.trim_end().len()).max(s);
            base + s..base + e
        })
        .collect()
}

fn push_mapped(
    segs: &mut Vec<Seg>,
    rendered: &str,
    source: &str,
    leaf_start: usize,
    src_start: usize,
    linear: bool,
) {
    if !linear || rendered.len() != source.len() || rendered == source {
        segs.push(Seg {
            leaf: leaf_start..leaf_start + rendered.len(),
            src: src_start..src_start + source.len(),
            linear: linear && rendered == source,
            line_end: false,
        });
        return;
    }
    let (r, s) = (rendered.as_bytes(), source.as_bytes());
    let mut run = 0;
    let mut i = 0;
    while i < r.len() {
        if r[i] == s[i] {
            i += 1;
            continue;
        }
        if run < i {
            segs.push(Seg {
                leaf: leaf_start + run..leaf_start + i,
                src: src_start + run..src_start + i,
                linear: true,
                line_end: false,
            });
        }
        let mut j = i + 1;
        while j < r.len() && !rendered.is_char_boundary(j) {
            j += 1;
        }
        segs.push(Seg {
            leaf: leaf_start + i..leaf_start + j,
            src: src_start + i..src_start + j,
            linear: false,
            line_end: matches!(s[i], b'\n' | b'\r'),
        });
        i = j;
        run = j;
    }
    if run < r.len() {
        segs.push(Seg {
            leaf: leaf_start + run..leaf_start + r.len(),
            src: src_start + run..src_start + r.len(),
            linear: true,
            line_end: false,
        });
    }
}

fn fill_gaps(
    mut segs: Vec<Seg>,
    len: usize,
    bounds: Option<Range<usize>>,
    empty_src: usize,
) -> Vec<Seg> {
    segs.sort_by_key(|s| s.leaf.start);
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + 2);
    let mut cursor = 0;
    let mut src_cursor = bounds.as_ref().map_or(empty_src, |b| b.start);
    for seg in segs {
        if seg.leaf.start < cursor {
            continue;
        }
        if seg.leaf.start > cursor {
            out.push(Seg {
                leaf: cursor..seg.leaf.start,
                src: src_cursor..seg.src.start.max(src_cursor),
                linear: false,
                line_end: false,
            });
        }
        cursor = seg.leaf.end;
        src_cursor = seg.src.end.max(src_cursor);
        out.push(seg);
    }
    if cursor < len {
        let end = bounds
            .as_ref()
            .map_or(src_cursor, |b| b.end.max(src_cursor));
        out.push(Seg {
            leaf: cursor..len,
            src: src_cursor..end,
            linear: false,
            line_end: false,
        });
    }
    let mut merged: Vec<Seg> = Vec::with_capacity(out.len());
    for seg in out {
        if let Some(last) = merged.last_mut()
            && last.linear
            && seg.linear
            && last.leaf.end == seg.leaf.start
            && last.src.end == seg.src.start
        {
            last.leaf.end = seg.leaf.end;
            last.src.end = seg.src.end;
            continue;
        }
        merged.push(seg);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_spans_split_on_unescaped_pipes() {
        let line = "| a | b \\| c |  |";
        let spans: Vec<&str> = cell_spans(line, 0)
            .iter()
            .map(|r| &line[r.clone()])
            .collect();
        assert_eq!(spans, ["a", "b \\| c", ""]);
        let line = "a | b";
        let spans: Vec<&str> = cell_spans(line, 0)
            .iter()
            .map(|r| &line[r.clone()])
            .collect();
        assert_eq!(spans, ["a", "b"]);
    }

    #[test]
    fn gaps_map_as_a_whole() {
        let segs = vec![Seg {
            leaf: 0..3,
            src: 0..3,
            linear: true,
            line_end: false,
        }];
        let out = fill_gaps(segs, 6, Some(0..10), 0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].leaf, 3..6);
        assert_eq!(out[1].src, 3..10);
    }
}
