//! The page as Live Preview edits it: the note's top-level blocks, and in
//! them every line of text that can be typed in (a paragraph, a heading, a
//! list item's text), each with the map from its drawn letters to the
//! Markdown. Built from one parse of the note; nothing here is saved.

use super::flow::{self, Flow};
use markdown::mdast::{List, ListItem, Node};
use std::ops::Range;

/// A line of text that's typed in where it's drawn.
#[derive(Clone, Debug)]
pub struct Leaf {
    /// Where the caret can be, in the note: the text's Markdown.
    pub span: Range<usize>,
    pub flow: Flow,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub enum Kind {
    /// A paragraph or heading; `block` is its whole range.
    Text { block: Range<usize> },
    Item(Item),
    /// A paragraph in a container: a quote, or a list item past its first
    /// paragraph. `lead` starts each of its lines (`> `, `  `, `  > `).
    Quote { lead: String },
    /// A code block's code, in `lang`; `lead` starts each of its lines
    /// inside a list item or quote.
    Code { lang: Option<String>, lead: String },
    /// A table cell's text: row (0 is the header) and column.
    Cell { row: usize, col: usize },
}

/// A list item: what its line starts with, so Enter can start the next one
/// alike and Tab can indent it.
#[derive(Clone, Debug)]
pub struct Item {
    /// The start of the item's first line.
    pub line_start: usize,
    /// The marker and checkbox before its text (`- [ ] `).
    pub prefix: Range<usize>,
    /// The checkbox (`[ ]` or `[x]`) and whether it's ticked.
    pub checkbox: Option<(Range<usize>, bool)>,
    /// The start of a new item like this one: indent, marker, checkbox.
    pub next_prefix: String,
    /// How far a nested item is indented under it.
    pub indent: usize,
    /// The whole item's range.
    pub range: Range<usize>,
}

/// A top-level block as the page draws it.
#[derive(Clone, Debug)]
pub enum Shape {
    /// A paragraph or heading: the leaf it is.
    Text(usize),
    /// A list, row by row (nested items included, in order).
    List(Vec<Row>),
    /// A quote: its paragraphs' leaves.
    Quote(Vec<usize>),
    /// A code block: its code's leaf.
    Code(usize),
    /// A table: its cells' leaves, row by row, how each column aligns, and
    /// where its last row ends (new rows go after).
    Table { rows: Vec<Vec<usize>>, aligns: Vec<Align>, end: usize },
    /// A footnote's text (`[^1]: …`): its leaf and its number.
    Note { leaf: usize, number: String },
    /// Anything else: drawn as the reader draws it, with its own controls.
    Other,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub leaf: usize,
    pub depth: usize,
    pub marker: Marker,
    /// In a quote: the quote's own depth, and whether it's inside a list
    /// item (then it starts at the item's text).
    pub quote: Option<(usize, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub enum Marker {
    Bullet,
    Number(u32),
    Task(bool),
    /// More of a list item (a paragraph, code): no marker, at its text.
    Continue,
    /// A line of a quote: no marker at all.
    Nothing,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    /// Each top-level block's range and shape.
    pub blocks: Vec<(Range<usize>, Shape)>,
    /// Every leaf, in reading order.
    pub leaves: Vec<Leaf>,
}

impl Page {
    #[cfg(test)]
    pub fn read(source: &str) -> Self {
        Self::read_with_raw(source, None)
    }

    /// The page, with the blocks in `raw` shown as their Markdown, edited
    /// as text (⌘⇧E).
    pub fn read_with_raw(source: &str, raw: Option<Range<usize>>) -> Self {
        let mut options = markdown::ParseOptions::gfm();
        options.constructs.math_flow = true;
        options.constructs.math_text = true;
        let Ok(tree) = markdown::to_mdast(source, &options) else { return Page::default() };
        // The note's `[id]: url` definitions, for reference links.
        let definitions = tree
            .children()
            .into_iter()
            .flatten()
            .filter_map(|node| match node {
                Node::Definition(d) => Some((d.identifier.to_lowercase(), d.url.clone())),
                _ => None,
            })
            .collect();
        // Footnotes are numbered in the order the note first marks them.
        let mut footnotes = std::collections::HashMap::new();
        fn number(node: &Node, footnotes: &mut std::collections::HashMap<String, usize>) {
            if let Node::FootnoteReference(note) = node {
                let next = footnotes.len() + 1;
                footnotes.entry(note.identifier.to_lowercase()).or_insert(next);
            }
            for child in node.children().into_iter().flatten() {
                number(child, footnotes);
            }
        }
        number(&tree, &mut footnotes);
        flow::with_definitions(definitions, footnotes, || Self::read_tree(source, &tree, raw))
    }

    fn read_tree(source: &str, tree: &Node, raw: Option<Range<usize>>) -> Self {
        let mut page = Page::default();
        let mut raw_placed = false;
        for node in tree.children().into_iter().flatten() {
            let Some(position) = node.position() else { continue };
            let range = position.start.offset..position.end.offset;
            // The raw Markdown: one block of text where these blocks were.
            if let Some(raw) = raw.as_ref().filter(|r| range.start < r.end.max(r.start + 1) && r.start < range.end.max(range.start + 1)) {
                if !raw_placed {
                    raw_placed = true;
                    let flow = Flow::plain(source, raw.clone());
                    page.leaves.push(Leaf { span: raw.clone(), flow, kind: Kind::Code { lang: Some("markdown".into()), lead: String::new() } });
                    page.blocks.push((raw.clone(), Shape::Code(page.leaves.len() - 1)));
                }
                continue;
            }
            let shape = match node {
                Node::Paragraph(p) => text(&mut page, source, None, &p.children, range.clone()),
                Node::Heading(h) => text(&mut page, source, Some(h.depth), &h.children, range.clone()),
                Node::List(list) => {
                    let mark = page.leaves.len();
                    let mut rows = Vec::new();
                    if list_rows(&mut page, source, list, 0, None, &mut rows).is_some() {
                        Shape::List(rows)
                    } else {
                        page.leaves.truncate(mark);
                        Shape::Other
                    }
                }
                Node::Blockquote(quote) => {
                    let mark = page.leaves.len();
                    let mut leaves = Vec::new();
                    let ok = quote.children.iter().all(|child| {
                        let Node::Paragraph(p) = child else { return false };
                        let Some(at) = p.position.as_ref() else { return false };
                        let Some(mut flow) = flow::from_inline(source, None, &p.children, at.end.offset) else {
                            return false;
                        };
                        let end = flow.take_trailing_spaces(source, at.end.offset);
                        let lead = lead_of(source, at.start.offset);
                        page.leaves.push(Leaf { span: at.start.offset..end, flow, kind: Kind::Quote { lead } });
                        leaves.push(page.leaves.len() - 1);
                        true
                    });
                    if ok && !leaves.is_empty() {
                        Shape::Quote(leaves)
                    } else {
                        // A quote holding lists or code: its lines as rows.
                        page.leaves.truncate(mark);
                        let mut rows = Vec::new();
                        if quote_rows(&mut page, source, quote, 0, false, &mut rows).is_some() && !rows.is_empty() {
                            Shape::List(rows)
                        } else {
                            page.leaves.truncate(mark);
                            Shape::Other
                        }
                    }
                }
                Node::Table(table) => {
                    let mark = page.leaves.len();
                    match table_rows(&mut page, source, table) {
                        Some(rows) => Shape::Table {
                            rows,
                            aligns: table
                                .align
                                .iter()
                                .map(|a| match a {
                                    markdown::mdast::AlignKind::Left => Align::Left,
                                    markdown::mdast::AlignKind::Center => Align::Center,
                                    markdown::mdast::AlignKind::Right => Align::Right,
                                    markdown::mdast::AlignKind::None => Align::None,
                                })
                                .collect(),
                            end: range.end,
                        },
                        None => {
                            page.leaves.truncate(mark);
                            Shape::Other
                        }
                    }
                }
                // A footnote of one paragraph: its text, after its number.
                Node::FootnoteDefinition(note) => match note.children.as_slice() {
                    [Node::Paragraph(p)] => {
                        let at = p.position.as_ref().map(|p| p.start.offset..p.end.offset);
                        match at.and_then(|at| flow::from_inline(source, None, &p.children, at.end).map(|f| (at, f))) {
                            Some((at, mut flow)) => {
                                let end = flow.take_trailing_spaces(source, at.end);
                                page.leaves.push(Leaf { span: at.start..end, flow, kind: Kind::Text { block: range.clone() } });
                                let number = flow::footnote_number(&note.identifier, note.label.as_deref());
                                Shape::Note { leaf: page.leaves.len() - 1, number }
                            }
                            None => Shape::Other,
                        }
                    }
                    _ => Shape::Other,
                },
                Node::Code(code) if code.lang.as_deref() != Some("mermaid") => match code_leaf(source, code) {
                    Some(leaf) => {
                        page.leaves.push(leaf);
                        Shape::Code(page.leaves.len() - 1)
                    }
                    None => Shape::Other,
                },
                _ => Shape::Other,
            };
            page.blocks.push((range, shape));
        }
        page
    }

    /// The leaf the caret at `at` is in.
    pub fn leaf_at(&self, at: usize) -> Option<usize> {
        self.leaves.iter().position(|l| l.span.start <= at && at <= l.span.end)
    }

    /// Whether `at` is in a top-level block (then not in a gap between them).
    pub fn in_block(&self, at: usize) -> bool {
        self.blocks.iter().any(|(r, _)| r.start <= at && at <= r.end)
    }
}

/// A table's cells as leaves, row by row; `None` if a cell holds something
/// not drawn as text yet.
fn table_rows(page: &mut Page, source: &str, table: &markdown::mdast::Table) -> Option<Vec<Vec<usize>>> {
    let mut rows = Vec::new();
    for (r, row) in table.children.iter().enumerate() {
        let Node::TableRow(row) = row else { return None };
        let mut cells = Vec::new();
        for (c, cell) in row.children.iter().enumerate() {
            let Node::TableCell(cell) = cell else { return None };
            let at = cell.position.as_ref()?;
            // The cell's range starts at its `|`; text goes after `| `.
            let raw = &source[at.start.offset..at.end.offset];
            let after_pipe = at.start.offset + usize::from(raw.starts_with('|'));
            let empty_at = after_pipe + usize::from(source[after_pipe..].starts_with(' ') && !source[after_pipe..].starts_with(" |"));
            let flow = flow::from_inline(source, None, &cell.children, empty_at)?;
            let start = cell.children.first().and_then(|n| n.position()).map_or(empty_at, |p| p.start.offset);
            let text_end = cell.children.last().and_then(|n| n.position()).map_or(empty_at, |p| p.end.offset);
            // The cell runs up to its closing `|`, so the caret stays in it on
            // the padding (which isn't drawn: aligned tables pad with spaces).
            let pipe = source[text_end..].find('|').map_or(text_end, |n| text_end + n);
            let span = start..pipe.max(start);
            page.leaves.push(Leaf { span, flow, kind: Kind::Cell { row: r, col: c } });
            cells.push(page.leaves.len() - 1);
        }
        rows.push(cells);
    }
    Some(rows)
}

/// What starts the line `at` is on, up to `at` (`> `, a list item's
/// indent).
fn lead_of(source: &str, at: usize) -> String {
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    source[line_start..at].to_string()
}

/// A fenced code block's code as a line of its own (`None` for Mermaid,
/// indented code, or code not written out as is).
fn code_leaf(source: &str, code: &markdown::mdast::Code) -> Option<Leaf> {
    let at = code.position.as_ref()?;
    let range = at.start.offset..at.end.offset;
    let lead = lead_of(source, range.start);
    let content = code_content(source, range, &code.value, &lead)?;
    let flow = Flow::plain_lead(source, content.clone(), &lead);
    Some(Leaf { span: content, flow, kind: Kind::Code { lang: code.lang.clone(), lead } })
}

/// A fenced code block's code, between its fences (`None` for indented
/// code, or code whose text isn't written out as is). In a container each
/// line starts with `lead`, which the code itself doesn't hold.
fn code_content(source: &str, range: Range<usize>, value: &str, lead: &str) -> Option<Range<usize>> {
    let block = &source[range.clone()];
    if !(block.starts_with("```") || block.starts_with("~~~")) {
        return None;
    }
    let start = range.start + block.find('\n')? + 1;
    let last_line = block.rfind('\n').map(|i| range.start + i + 1).unwrap_or(range.end);
    let closed = last_line > start - 1 && {
        let line = source[last_line..range.end].trim();
        line.starts_with("```") || line.starts_with("~~~")
    };
    let end = if closed { last_line.saturating_sub(1).max(start) } else { range.end };
    let content = start..end.max(start);
    // The first line's lead comes before the code.
    let content = if !lead.is_empty() && source[content.clone()].starts_with(lead) { content.start + lead.len()..content.end.max(content.start + lead.len()) } else { content };
    let written: String = if lead.is_empty() {
        source[content.clone()].to_string()
    } else {
        let text = &source[content.clone()];
        let mut out = String::new();
        for (n, line) in text.split('\n').enumerate() {
            if n > 0 {
                out.push('\n');
                let common = line.bytes().zip(lead.bytes()).take_while(|(a, b)| a == b).count();
                out.push_str(&line[common..]);
            } else {
                out.push_str(line);
            }
        }
        out
    };
    (written == *value).then_some(content)
}

fn text(page: &mut Page, source: &str, heading: Option<u8>, children: &[Node], range: Range<usize>) -> Shape {
    let end = range.end;
    match flow::from_inline(source, heading, children, end) {
        Some(mut flow) => {
            let end = flow.take_trailing_spaces(source, end);
            page.leaves.push(Leaf { span: range.start..end, flow, kind: Kind::Text { block: range } });
            Shape::Text(page.leaves.len() - 1)
        }
        None => Shape::Other,
    }
}

/// A list's items as rows, what's nested in each after it: more
/// paragraphs, code, quotes, lists. `quote`: the quote the list is in.
/// `None` when an item holds something else (a table, HTML…).
fn list_rows(page: &mut Page, source: &str, list: &List, depth: usize, quote: Option<(usize, bool)>, rows: &mut Vec<Row>) -> Option<()> {
    let mut number = list.start.unwrap_or(1);
    for child in &list.children {
        let Node::ListItem(item) = child else { return None };
        let leaf = item_leaf(source, item)?;
        let task = match &leaf.leaf.kind {
            Kind::Item(item) => item.checkbox.as_ref().map(|(_, done)| *done),
            _ => None,
        };
        page.leaves.push(leaf.leaf);
        let marker = match task {
            Some(done) => Marker::Task(done),
            None if list.ordered => Marker::Number(number),
            None => Marker::Bullet,
        };
        number += 1;
        rows.push(Row { leaf: page.leaves.len() - 1, depth, marker, quote });
        let first_is_text = matches!(item.children.first(), Some(Node::Paragraph(_)));
        for nested in item.children.iter().skip(usize::from(first_is_text)) {
            match nested {
                Node::List(nested) => list_rows(page, source, nested, depth + 1, quote, rows)?,
                Node::Paragraph(p) => {
                    let leaf = paragraph_leaf(source, p)?;
                    page.leaves.push(leaf);
                    rows.push(Row { leaf: page.leaves.len() - 1, depth, marker: Marker::Continue, quote });
                }
                Node::Code(code) => {
                    page.leaves.push(code_leaf(source, code)?);
                    rows.push(Row { leaf: page.leaves.len() - 1, depth, marker: Marker::Continue, quote });
                }
                Node::Blockquote(inner) => quote_rows(page, source, inner, depth, true, rows)?,
                _ => return None,
            }
        }
    }
    Some(())
}

/// A paragraph in a container, as a line that keeps the container's lead.
fn paragraph_leaf(source: &str, p: &markdown::mdast::Paragraph) -> Option<Leaf> {
    let at = p.position.as_ref()?;
    let mut flow = flow::from_inline(source, None, &p.children, at.end.offset)?;
    let end = flow.take_trailing_spaces(source, at.end.offset);
    Some(Leaf { span: at.start.offset..end, flow, kind: Kind::Quote { lead: lead_of(source, at.start.offset) } })
}

/// A quote's lines as rows: paragraphs, lists, code. `depth` and `in_item`
/// say where it starts (at a list item's text, or the page's edge).
fn quote_rows(page: &mut Page, source: &str, quote: &markdown::mdast::Blockquote, depth: usize, in_item: bool, rows: &mut Vec<Row>) -> Option<()> {
    let at = Some((depth, in_item));
    for child in &quote.children {
        match child {
            Node::Paragraph(p) => {
                page.leaves.push(paragraph_leaf(source, p)?);
                let marker = if in_item { Marker::Continue } else { Marker::Nothing };
                rows.push(Row { leaf: page.leaves.len() - 1, depth, marker, quote: at });
            }
            Node::Code(code) => {
                page.leaves.push(code_leaf(source, code)?);
                let marker = if in_item { Marker::Continue } else { Marker::Nothing };
                rows.push(Row { leaf: page.leaves.len() - 1, depth, marker, quote: at });
            }
            Node::List(list) => list_rows(page, source, list, depth + usize::from(in_item), at, rows)?,
            _ => return None,
        }
    }
    Some(())
}

struct ItemLeaf {
    leaf: Leaf,
}

fn item_leaf(source: &str, item: &ListItem) -> Option<ItemLeaf> {
    let position = item.position.as_ref()?;
    let range = position.start.offset..position.end.offset;
    let line_start = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let mut paragraph = match item.children.first() {
        Some(Node::Paragraph(p)) => Some(p),
        Some(Node::List(_)) | None => None,
        Some(_) => return None,
    };
    // `- [ ] ` with nothing typed yet reads as text "[ ]": it's an empty task.
    let mut checked = item.checked;
    if checked.is_none()
        && let Some(p) = paragraph
        && let Some(at) = p.position.as_ref()
    {
        let raw = source[at.start.offset..at.end.offset].trim();
        if matches!(raw, "[ ]" | "[x]" | "[X]") {
            checked = Some(raw != "[ ]");
            paragraph = None;
        }
    }
    // The text starts after the marker and checkbox.
    let content_start = match paragraph.and_then(|p| p.position.as_ref()) {
        Some(p) => p.start.offset,
        None => {
            // An empty item: just after its marker (and checkbox), on its line.
            let line_end = source[range.start..].find('\n').map_or(source.len(), |i| range.start + i);
            line_end.min(range.end.max(range.start)).max(range.start)
        }
    };
    let prefix = range.start..content_start;
    let marker_text = &source[prefix.clone()];
    let checkbox = checked.and_then(|done| {
        let open = marker_text.find('[')?;
        Some((range.start + open..range.start + open + 3, done))
    });
    // The marker itself (`-`, `*`, `1.`), for the next item.
    let marker = marker_text.split_whitespace().next().unwrap_or("-");
    let next_marker = match marker.trim_end_matches(['.', ')']).parse::<u32>() {
        Ok(n) => format!("{}{}", n + 1, &marker[marker.len() - 1..]),
        Err(_) => marker.to_string(),
    };
    let indent_text = &source[line_start..range.start];
    let next_prefix = format!(
        "{indent_text}{next_marker} {}",
        if checkbox.is_some() { "[ ] " } else { "" }
    );
    let mut flow = match paragraph {
        Some(p) => flow::from_inline(source, None, &p.children, content_start)?,
        None => Flow::empty(content_start),
    };
    let span_end = paragraph
        .and_then(|p| p.position.as_ref())
        .map_or(content_start, |p| p.end.offset);
    let span_end = if paragraph.is_some() { flow.take_trailing_spaces(source, span_end) } else { span_end };
    Some(ItemLeaf {
        leaf: Leaf {
            span: content_start..span_end,
            flow,
            kind: Kind::Item(Item {
                line_start,
                prefix,
                checkbox,
                next_prefix,
                indent: marker.len() + 1,
                range,
            }),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_blocks_become_rows() {
        let source = "> A quote with a list:\n>\n> - item one\n> - item two\n\n- A list item with code:\n\n  ```sh\n  cargo run\n  cargo test\n  ```\n\n- A list item with a quote:\n\n  > quoted inside\n  > the item\n\n  And more text.\n";
        let page = Page::read(source);
        let texts: Vec<_> = page.leaves.iter().map(|l| l.flow.text.as_str()).collect();
        assert_eq!(
            texts,
            ["A quote with a list:", "item one", "item two", "A list item with code:", "cargo run\ncargo test", "A list item with a quote:", "quoted inside the item", "And more text."]
        );
        let Shape::List(rows) = &page.blocks[0].1 else { panic!("{:?}", page.blocks[0].1) };
        assert!(matches!(rows[0].marker, Marker::Nothing) && rows[0].quote == Some((0, false)));
        assert!(matches!(rows[1].marker, Marker::Bullet) && rows[1].quote == Some((0, false)));
        let Kind::Item(item) = &page.leaves[1].kind else { panic!() };
        assert_eq!(item.next_prefix, "> - ");
        // Code in an item: its indent isn't drawn, and goes with the break.
        let code = &page.leaves[4];
        assert!(matches!(&code.kind, Kind::Code { lead, .. } if lead == "  "));
        let at = code.flow.text.find('\n').unwrap();
        assert_eq!(&source[code.flow.delete_after(at).unwrap()], "\n  ");
        // A quote in an item, and a later paragraph of it.
        assert!(matches!(&page.leaves[6].kind, Kind::Quote { lead } if lead == "  > "));
        assert!(matches!(&page.leaves[7].kind, Kind::Quote { lead } if lead == "  "));
        let Shape::List(rows) = &page.blocks[1].1 else { panic!() };
        assert_eq!(rows[3].quote, Some((0, true)));
        assert!(matches!(rows[4].marker, Marker::Continue) && rows[4].quote.is_none());
    }

    #[test]
    fn lists_become_rows_of_leaves() {
        let source = "# Plan\n\n- [ ] pack **bags**\n- [x] book\n  - nested\n1. one\n\n| a |\n|---|\n";
        let page = Page::read(source);
        let texts: Vec<_> = page.leaves.iter().map(|l| l.flow.text.as_str()).collect();
        assert_eq!(texts, ["Plan", "pack bags", "book", "nested", "one", "a"]);
        let Shape::List(rows) = &page.blocks[1].1 else { panic!("{:?}", page.blocks[1].1) };
        assert!(matches!(rows[0].marker, Marker::Task(false)));
        assert!(matches!(rows[1].marker, Marker::Task(true)));
        assert_eq!(rows[2].depth, 1);
        // The table's cells are lines of their own.
        assert!(matches!(page.blocks.last().unwrap().1, Shape::Table { .. }));
        let Kind::Item(item) = &page.leaves[1].kind else { panic!() };
        assert_eq!(&source[item.checkbox.clone().unwrap().0], "[ ]");
        assert_eq!(item.next_prefix, "- [ ] ");
        // The caret in an item's text finds that item.
        let at = source.find("bags").unwrap();
        assert_eq!(page.leaf_at(at), Some(1));
    }
}
