//! ⌘/ in Live Preview: what can be done to the thing under the caret.
//!
//! The menu is built for where the caret is: in bold text it offers
//! Remove Bold (and the other marks), in a highlight its colours, on a link
//! Edit Link and Remove Link, in a list item Turn Into and Mark as Done, in
//! a table cell the table's rows and columns, on a picked image Delete. To
//! give a new component its own actions, add a group to [`LiveView::actions`]
//! and its [`Act`] to [`LiveView::run_action`].

use super::flow::Wrap;
use super::page::{Align, Kind, Shape};
use super::{LiveView, Spot, TableOp};
use crate::rich;
use gpui_kit::*;
use std::ops::Range;

/// One thing the menu can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Bold, italic… on the selection (or the word), or off the run it's in.
    Toggle(Wrap),
    /// `<u>` likewise.
    Underline,
    /// Every mark around or in the selection (or the word) gone.
    ClearFormatting,
    /// The selection as inline math (`$…$`).
    Math,
    Case(Case),
    /// A highlight colour: the highlight recoloured, or the selection
    /// highlighted.
    Highlight(u32),
    /// The highlight card, held open for its colour picker.
    CustomColor,
    RemoveHighlight,
    EditLink,
    RemoveLink,
    OpenLink(String),
    EditMath,
    TurnInto(Into),
    ToggleDone,
    Table(TableOp),
    /// The block as its Markdown (⌘⇧E).
    EditMarkdown,
    Duplicate,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Sentence,
}

/// What a line can become.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Into {
    Text,
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Quote,
    Code,
}

impl Into {
    /// What this kind of line starts with in Markdown.
    fn prefix(self) -> String {
        match self {
            Into::Text | Into::Code => String::new(),
            Into::Heading(level) => format!("{} ", "#".repeat(level as usize)),
            Into::Bullet => "- ".into(),
            Into::Numbered => "1. ".into(),
            Into::Task => "- [ ] ".into(),
            Into::Quote => "> ".into(),
        }
    }
}

/// A button in the menu's row of formatting icons.
#[derive(Clone, Debug)]
pub struct Button {
    pub act: Act,
    pub icon: gpui_kit::assets::IconName,
    pub label: &'static str,
    /// What it toggles is already on here.
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub group: &'static str,
    pub label: String,
    pub keywords: &'static [&'static str],
    pub act: Act,
    /// A colour shown beside it (a highlight swatch).
    pub swatch: Option<Hsla>,
}

fn action(group: &'static str, label: impl std::convert::Into<String>, keywords: &'static [&'static str], act: Act) -> Action {
    Action { group, label: label.into(), keywords, act, swatch: None }
}

/// Where the menu was opened: restored before an action runs, since the
/// menu's search field takes the focus (and with it the caret) meanwhile.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub caret: Option<usize>,
    pub anchor: Option<usize>,
    pub picked: Option<usize>,
}

impl LiveView {
    /// The selection's (or caret's) drawn range in line `leaf`, when it's in
    /// that one line.
    fn shown_in(&self, leaf: usize) -> Option<Range<usize>> {
        let flow = &self.page.leaves[leaf].flow;
        let caret = self.caret?;
        match self.selection() {
            Some(sel) if self.page.leaf_at(sel.start) == Some(leaf) && self.page.leaf_at(sel.end) == Some(leaf) => {
                Some(flow.shown_for(sel.start)..flow.shown_for(sel.end))
            }
            Some(_) => None,
            None => {
                let at = flow.shown_for(caret);
                Some(at..at)
            }
        }
    }

    /// What ⌘/ offers here, group by group.
    pub(super) fn actions(&self) -> Vec<Action> {
        let mut out = Vec::new();
        // A picked image (or diagram, rule…): the block as a whole.
        if self.picked_range().is_some() {
            out.push(action("Block", "Edit as Markdown", &["raw", "source", "markdown"], Act::EditMarkdown));
            out.push(action("Block", "Duplicate", &["copy", "clone"], Act::Duplicate));
            out.push(action("Block", "Delete", &["remove"], Act::Delete));
            return out;
        }
        let Some(Spot::Leaf(i)) = self.spot() else { return out };
        let leaf = &self.page.leaves[i];
        let flow = &leaf.flow;
        let code = matches!(leaf.kind, Kind::Code { .. });
        if let Some(shown) = self.shown_in(i).filter(|_| !code) {
            let selected = !shown.is_empty();
            // The row of buttons above the list, as rows too, to find by
            // typing: Remove when it's on here, else add.
            for button in self.buttons() {
                let (on, off, keywords): (&str, String, &'static [&'static str]) = match button.act {
                    Act::Toggle(Wrap::Strong) => ("Bold", "Remove Bold".into(), &["strong", "b"]),
                    Act::Toggle(Wrap::Emphasis) => ("Italic", "Remove Italic".into(), &["emphasis", "i"]),
                    Act::Underline => ("Underline", "Remove Underline".into(), &["u"]),
                    Act::Toggle(Wrap::Delete) => ("Strikethrough", "Remove Strikethrough".into(), &["strike", "cross"]),
                    Act::Toggle(Wrap::Code) => ("Inline Code", "Remove Inline Code".into(), &["monospace", "code"]),
                    Act::EditLink => ("Link…", "Edit Link".into(), &["url", "address"]),
                    Act::Math | Act::EditMath => ("Math", "Edit Math".into(), &["tex", "latex", "equation", "formula"]),
                    Act::ClearFormatting => ("Clear Formatting", String::new(), &["plain", "remove", "reset"]),
                    // Highlight has its own group below.
                    _ => continue,
                };
                let label = if button.active { off } else { on.to_string() };
                out.push(action("Format", label, keywords, button.act));
            }
            // A highlight: its colours. A selection: highlight it.
            let mark = flow.mark_at(shown.start, false).filter(|m| {
                let inner = flow.shown_for(m.open.end)..flow.shown_for(m.close.start);
                inner.start <= shown.start && shown.end <= inner.end
            });
            if mark.is_some() || selected {
                let current = mark.as_ref().map(|m| rich::highlight_hex(m.color));
                for (name, value) in rich::HIGHLIGHTS {
                    let swatch: Hsla = rgb(value).into();
                    if current.as_deref() == Some(rich::highlight_hex(swatch).as_str()) {
                        continue;
                    }
                    let mut a = action("Highlight", name, &["highlight", "mark", "color", "colour"], Act::Highlight(value));
                    a.swatch = Some(swatch);
                    out.push(a);
                }
                out.push(action("Highlight", "Custom Color…", &["highlight", "picker", "color", "colour"], Act::CustomColor));
                if mark.is_some() {
                    out.push(action("Highlight", "Remove Highlight", &["mark", "color", "colour"], Act::RemoveHighlight));
                }
            }
            // A link: edit, open or remove it. A selection: link it.
            if let Some(link) = flow.link_at(shown.start).filter(|_| flow.link_at(shown.end).is_some()) {
                if !link.wiki {
                    out.push(action("Link", "Open Link", &["browser", "visit"], Act::OpenLink(link.url.clone())));
                }
                out.push(action("Link", "Remove Link", &["unlink"], Act::RemoveLink));
            }
            // Case: of the selection, or the word at the caret.
            for (label, case) in
                [("UPPERCASE", Case::Upper), ("lowercase", Case::Lower), ("Title Case", Case::Title), ("Sentence case", Case::Sentence)]
            {
                out.push(action("Case", label, &["case", "capital", "upper", "lower"], Act::Case(case)));
            }
        }
        // The line: what it can become.
        let current = match &leaf.kind {
            Kind::Text { .. } => Some(match flow.heading {
                Some(level) => Into::Heading(level),
                None => Into::Text,
            }),
            Kind::Item(item) if item.checkbox.is_some() => Some(Into::Task),
            Kind::Item(item) if self.source[item.prefix.clone()].trim_start().starts_with(|c: char| c.is_ascii_digit()) => {
                Some(Into::Numbered)
            }
            Kind::Item(_) => Some(Into::Bullet),
            Kind::Quote => Some(Into::Quote),
            _ => None,
        };
        let note = self.page.blocks.iter().any(|(_, s)| matches!(s, Shape::Note { leaf, .. } if *leaf == i));
        if let Some(current) = current.filter(|_| !note) {
            let nested = matches!(&leaf.kind, Kind::Item(item) if item.prefix.start > item.line_start);
            for (label, into, keywords) in [
                ("Text", Into::Text, &["paragraph", "body", "plain"][..]),
                ("Heading 1", Into::Heading(1), &["title", "h1"][..]),
                ("Heading 2", Into::Heading(2), &["h2", "subtitle"][..]),
                ("Heading 3", Into::Heading(3), &["h3"][..]),
                ("Bullet List", Into::Bullet, &["unordered", "points"][..]),
                ("Numbered List", Into::Numbered, &["ordered", "steps"][..]),
                ("Checkbox", Into::Task, &["todo", "task"][..]),
                ("Quote", Into::Quote, &["blockquote"][..]),
                ("Code Block", Into::Code, &["snippet", "fence"][..]),
            ] {
                let fits = match into {
                    // Headings and quotes stand on their own lines.
                    Into::Heading(_) | Into::Quote => !nested,
                    Into::Code => current == Into::Text || matches!(current, Into::Heading(_)),
                    _ => true,
                };
                if into != current && fits {
                    out.push(action("Turn Into", label, keywords, Act::TurnInto(into)));
                }
            }
        }
        match &leaf.kind {
            Kind::Item(item) if item.checkbox.is_some() => {
                let done = item.checkbox.as_ref().is_some_and(|(_, done)| *done);
                let label = if done { "Mark as Not Done" } else { "Mark as Done" };
                out.push(action("Item", label, &["tick", "check", "complete", "task"], Act::ToggleDone));
            }
            Kind::Cell { .. } => {
                for (label, op) in [
                    ("Insert Row Above", TableOp::RowAbove),
                    ("Insert Row Below", TableOp::RowBelow),
                    ("Insert Column Left", TableOp::ColumnLeft),
                    ("Insert Column Right", TableOp::ColumnRight),
                    ("Align Column Left", TableOp::Align(Align::Left)),
                    ("Align Column Center", TableOp::Align(Align::Center)),
                    ("Align Column Right", TableOp::Align(Align::Right)),
                    ("Delete Row", TableOp::DeleteRow),
                    ("Delete Column", TableOp::DeleteColumn),
                ] {
                    out.push(action("Table", label, &["table", "row", "column", "cell"], Act::Table(op)));
                }
            }
            _ => {}
        }
        // The block as a whole.
        if !matches!(leaf.kind, Kind::Cell { .. }) {
            if self.raw.is_none() {
                out.push(action("Block", "Edit as Markdown", &["raw", "source", "markdown"], Act::EditMarkdown));
            }
            out.push(action("Block", "Duplicate", &["copy", "clone"], Act::Duplicate));
            out.push(action("Block", "Delete", &["remove"], Act::Delete));
        }
        out
    }

    /// The menu's row of formatting buttons, when the caret is in text:
    /// each lit when what it toggles is already on there.
    pub(super) fn buttons(&self) -> Vec<Button> {
        let Some(Spot::Leaf(i)) = self.spot() else { return Vec::new() };
        if matches!(self.page.leaves[i].kind, Kind::Code { .. }) || self.picked.is_some() {
            return Vec::new();
        }
        let Some(shown) = self.shown_in(i) else { return Vec::new() };
        let flow = &self.page.leaves[i].flow;
        let wraps = flow.wraps_over(shown.clone());
        let on = |kind: Wrap| wraps.contains(&kind);
        use gpui_kit::assets::IconName;
        let link = flow.link_at(shown.start).is_some() && flow.link_at(shown.end).is_some();
        let math = shown.is_empty() && flow.math_at(shown.start).is_some();
        let mark = flow.mark_at(shown.start, false).is_some() && flow.mark_at(shown.end, false).is_some();
        let button = |act, icon, label, active| Button { act, icon, label, active };
        vec![
            button(Act::Toggle(Wrap::Strong), IconName::Bold, "Bold", on(Wrap::Strong)),
            button(Act::Toggle(Wrap::Emphasis), IconName::Italic, "Italic", on(Wrap::Emphasis)),
            button(Act::Underline, IconName::Underline, "Underline", flow.tag_over(shown.clone(), "u").is_some()),
            button(Act::Toggle(Wrap::Delete), IconName::Strikethrough, "Strikethrough", on(Wrap::Delete)),
            button(Act::Toggle(Wrap::Code), IconName::Code, "Inline Code", on(Wrap::Code)),
            button(Act::EditLink, IconName::Link, "Link", link),
            button(Act::CustomColor, IconName::Highlighter, "Highlight", mark),
            button(if math { Act::EditMath } else { Act::Math }, IconName::SquareRadical, "Math", math),
            button(Act::ClearFormatting, IconName::RemoveFormatting, "Clear Formatting", false),
        ]
    }

    /// The selection (or the word at the caret) in line `i`, drawn.
    fn selection_or_word(&self, i: usize) -> Option<Range<usize>> {
        let shown = self.shown_in(i)?;
        if !shown.is_empty() {
            return Some(shown);
        }
        let flow = &self.page.leaves[i].flow;
        let end = flow.word_end(shown.start);
        Some(flow.word_start(end)..end)
    }

    /// `<u>` on the selection (or the word), or off the run it's in.
    fn underline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Some(shown) = self.selection_or_word(i) else { return };
        let flow = self.page.leaves[i].flow.clone();
        let (caret, anchor) = (self.caret, self.anchor);
        if let Some((open, close)) = flow.tag_over(shown.clone(), "u") {
            self.edit(close.clone(), "", window, cx);
            self.last_edit = Some(std::time::Instant::now());
            self.edit(open.clone(), "", window, cx);
            let keep = |at: usize| {
                let at = if at >= close.end { at - close.len() } else { at.min(close.start) };
                if at >= open.end { at - open.len() } else { at.min(open.start) }
            };
            self.caret = caret.map(keep);
            self.anchor = anchor.map(keep);
        } else {
            let Some(span) = flow.char_span(shown) else { return };
            self.edit(span.end..span.end, "</u>", window, cx);
            self.last_edit = Some(std::time::Instant::now());
            self.edit(span.start..span.start, "<u>", window, cx);
            let keep = |at: usize| if at >= span.end { at + 7 } else if at >= span.start { at + 3 } else { at };
            self.caret = caret.map(keep);
            self.anchor = anchor.map(keep);
        }
        cx.notify();
    }

    /// Every mark around or in the selection (or the word) gone; its text
    /// (and links) stay.
    fn clear_formatting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Some(shown) = self.selection_or_word(i) else { return };
        let flow = self.page.leaves[i].flow.clone();
        let gone = flow.clear_edits(shown);
        if gone.is_empty() {
            return;
        }
        let (caret, anchor) = (self.caret, self.anchor);
        for (n, range) in gone.iter().enumerate() {
            if n > 0 {
                self.last_edit = Some(std::time::Instant::now());
            }
            self.edit(range.clone(), "", window, cx);
        }
        // Each place less what went before it.
        let keep = |at: usize| {
            gone.iter().fold(at, |at, r| if at >= r.end { at - r.len() } else if at > r.start { r.start } else { at })
        };
        self.caret = caret.map(keep);
        self.anchor = anchor.map(keep);
        cx.notify();
    }

    /// The selection as inline math: its text between `$`s.
    fn make_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Some(shown) = self.selection_or_word(i).filter(|s| !s.is_empty()) else { return };
        let flow = self.page.leaves[i].flow.clone();
        let Some(span) = flow.char_span(shown.clone()) else { return };
        let tex = flow.text[shown].to_string();
        self.edit(span.clone(), &format!("${tex}$"), window, cx);
        self.anchor = None;
        self.caret = Some(span.start + tex.len() + 2);
        cx.notify();
    }

    /// The whole of what the caret's line belongs to, for Duplicate and
    /// Delete: a list item (with what's nested in it), else the block.
    fn action_block(&self) -> Option<Range<usize>> {
        if let Some(range) = self.picked_range() {
            return Some(range);
        }
        let Some(Spot::Leaf(i)) = self.spot() else { return None };
        if let Kind::Item(item) = &self.page.leaves[i].kind {
            let end = self.source[item.range.end..].find('\n').map_or(self.source.len(), |n| item.range.end + n);
            let end = if self.source[item.range.clone()].ends_with('\n') { item.range.end } else { end };
            return Some(item.line_start..end.max(item.range.end));
        }
        let at = self.caret?;
        self.page.blocks.iter().find(|(r, _)| r.start <= at && at <= r.end).map(|(r, _)| r.clone())
    }

    /// Run `act` where the menu was opened.
    pub(super) fn run_action(&mut self, act: Act, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_page(window, cx);
        self.caret = scope.caret;
        self.anchor = scope.anchor;
        self.picked = scope.picked;
        self.parked = None;
        match act {
            Act::Toggle(kind) => self.toggle_mark(kind, window, cx),
            Act::Underline => self.underline(window, cx),
            Act::ClearFormatting => self.clear_formatting(window, cx),
            Act::Math => self.make_math(window, cx),
            Act::Case(case) => self.change_case(case, window, cx),
            Act::Highlight(value) => self.highlight(Some(rgb(value).into()), window, cx),
            Act::CustomColor => self.highlight(None, window, cx),
            Act::RemoveHighlight => {
                if self.open_mark_card_at_caret(false, window, cx) {
                    self.recolor_mark(None, window, cx);
                }
            }
            Act::EditLink => self.edit_link(window, cx),
            Act::RemoveLink => {
                let Some(Spot::Leaf(i)) = self.spot() else { return };
                let flow = self.page.leaves[i].flow.clone();
                let Some(link) = self.caret.and_then(|c| flow.link_at(flow.shown_for(c))) else { return };
                // The link's text as written (its own marks kept).
                let inner = flow.char_span(flow.shown_for(link.source.start)..flow.shown_for(link.source.end));
                let inner = inner.map(|r| self.source[r].to_string()).unwrap_or(link.text.clone());
                self.edit(link.source.clone(), &inner, window, cx);
                self.caret = Some(link.source.start + inner.len());
                cx.notify();
            }
            Act::OpenLink(url) => self.open_link(&url, window, cx),
            Act::EditMath => self.edit_link(window, cx),
            Act::TurnInto(into) => self.turn_into(into, window, cx),
            Act::ToggleDone => self.toggle_done(window, cx),
            Act::Table(op) => {
                let Some(Spot::Leaf(i)) = self.spot() else { return };
                let Kind::Cell { row, col } = self.page.leaves[i].kind else { return };
                let start = self.page.blocks.iter().find_map(|(range, shape)| match shape {
                    Shape::Table { rows, .. } if rows.iter().flatten().any(|l| *l == i) => Some(range.start),
                    _ => None,
                });
                if let Some(start) = start {
                    self.table_op(start, row, col, op, window, cx);
                }
            }
            Act::EditMarkdown => self.toggle_markdown(&super::ToggleMarkdown, window, cx),
            Act::Duplicate => {
                let Some(range) = self.action_block() else { return };
                let item = matches!(self.spot(), Some(Spot::Leaf(i)) if matches!(self.page.leaves[i].kind, Kind::Item(_)))
                    && self.picked.is_none();
                let copy = self.source[range.clone()].trim_end_matches('\n').to_string();
                let gap = if item { "\n" } else { "\n\n" };
                let at = range.start + copy.len();
                let offset = self.caret.map(|c| c.saturating_sub(range.start));
                self.picked = None;
                self.edit(at..at, &format!("{gap}{copy}"), window, cx);
                // The caret goes to the copy, at the same place in it.
                let start = at + gap.len();
                self.caret = Some(offset.map_or(start, |o| (start + o).min(start + copy.len())));
                self.anchor = None;
                cx.notify();
            }
            Act::Delete => {
                let Some(range) = self.action_block() else { return };
                self.remove_block(range, true, window, cx);
            }
        }
    }

    /// The selection (or the word at the caret) in a new case, letter by
    /// letter so its marks stay as written.
    fn change_case(&mut self, case: Case, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Some(mut shown) = self.shown_in(i) else { return };
        let flow = self.page.leaves[i].flow.clone();
        let selected = !shown.is_empty();
        if !selected {
            let end = flow.word_end(shown.start);
            shown = flow.word_start(end)..end;
        }
        let start = shown.start;
        let sentence_start = flow.text[..start].trim_end().is_empty() || flow.text[..start].trim_end().ends_with(['.', '!', '?']);
        let recase = |c: char, before: Option<char>| -> String {
            let word_start = before.is_none_or(|b| !b.is_alphanumeric() && b != '\'' && b != '’');
            match case {
                Case::Upper => c.to_uppercase().collect(),
                Case::Lower => c.to_lowercase().collect(),
                Case::Title if word_start => c.to_uppercase().collect(),
                Case::Title => c.to_lowercase().collect(),
                Case::Sentence => c.to_lowercase().collect(),
            }
        };
        // Sentence case: lower everything, then raise each sentence's first
        // letter.
        let edit = if case == Case::Sentence {
            let mut first = sentence_start;
            let mut after_stop = false;
            flow.recase(&self.source, shown.clone(), |c, before| {
                if before.is_some_and(|b| matches!(b, '.' | '!' | '?')) {
                    after_stop = true;
                }
                if c.is_alphabetic() && (first || after_stop) {
                    first = false;
                    after_stop = false;
                    return c.to_uppercase().collect();
                }
                if c.is_alphabetic() {
                    first = false;
                }
                recase(c, before)
            })
        } else {
            flow.recase(&self.source, shown.clone(), recase)
        };
        let Some((range, text)) = edit else { return };
        let (caret, anchor) = (self.caret, self.anchor);
        self.edit(range.clone(), &text, window, cx);
        // Same letters, same places (unless a letter grew, as ß to SS).
        let grown = text.len() as isize - range.len() as isize;
        let keep = |at: usize| if at >= range.end { (at as isize + grown) as usize } else { at.min(range.start + text.len()) };
        self.caret = caret.map(keep);
        self.anchor = anchor.map(keep);
        cx.notify();
    }

    /// Highlight: the highlight at the caret recoloured, or the selection
    /// wrapped in one. `None`: the card with its colour picker, held open.
    fn highlight(&mut self, color: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_mark_card_at_caret(color.is_none(), window, cx) {
            if color.is_some() {
                self.recolor_mark(color, window, cx);
            }
            return;
        }
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Some(shown) = self.selection_or_word(i).filter(|s| !s.is_empty()) else { return };
        let flow = self.page.leaves[i].flow.clone();
        let Some(span) = flow.char_span(shown.clone()) else { return };
        let hex = rich::highlight_hex(color.unwrap_or_else(|| rgb(rich::HIGHLIGHTS[0].1).into()));
        let open = format!("<mark color=\"{hex}\">");
        self.edit(span.end..span.end, "</mark>", window, cx);
        self.last_edit = Some(std::time::Instant::now());
        self.edit(span.start..span.start, &open, window, cx);
        self.anchor = Some(span.start + open.len());
        self.caret = Some(span.end + open.len());
        if color.is_none() {
            self.open_mark_card_at_caret(true, window, cx);
        }
        cx.notify();
    }

    /// The line becomes `into`: its leading Markdown (`# `, `- `, `> `…)
    /// swapped for the new kind's.
    fn turn_into(&mut self, into: Into, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let leaf = self.page.leaves[i].clone();
        if into == Into::Code {
            if matches!(leaf.kind, Kind::Text { .. }) {
                self.code_block_line(window, cx);
            }
            return;
        }
        let prefix = match &leaf.kind {
            Kind::Text { block } => block.start..leaf.flow.source_for(0).max(block.start),
            Kind::Item(item) => item.prefix.clone(),
            Kind::Quote => {
                let start = leaf.span.start;
                self.source[..start].rfind('\n').map_or(0, |n| n + 1)..start
            }
            _ => return,
        };
        self.edit_keeping_caret(prefix, &into.prefix(), window, cx);
    }
}
