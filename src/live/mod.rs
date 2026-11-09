//! Live Preview: the page, edited where it stands, as in Notion.
//!
//! The note's Markdown stays the one source of truth (the document's editor
//! holds it, so saving, undo, disk sync and search are unchanged), and its
//! Markdown is never shown. Every line of text (a paragraph, a heading, a
//! list item) is drawn as the reader draws it and typed in right there: a
//! map from each drawn letter to the Markdown turns each keystroke into an
//! edit of the note at that spot, so everything else stays as written.
//! Checkboxes tick, bullets and numbers stay bullets and numbers. Blocks
//! without text editing yet (tables, images, code, Mermaid) stay drawn with
//! their own controls.

mod element;
mod flow;
mod page;

use crate::{Enso, rich, typography};
use element::{GAP_LEAF, LiveElement, leaf_key};
use flow::{Flow, LinkAt, Wrap};
use gpui_kit::base::text::editor_host::{
    BlockNode, CodeBlock, ColumnAlign, FlowPosition, InlineNode, LeafGeometry, Paragraph, RenderContext, Table,
};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{EditorState, Input, InputEvent, InputState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::prelude::FluentBuilder;
use page::{Align, Kind, Marker, Page, Shape};
use std::{cell::RefCell, collections::HashMap, ops::Range, path::PathBuf, rc::Rc, time::Instant};

actions!(
    live,
    [
        Backspace, Delete, Enter, SoftBreak, Left, Right, Up, Down, Home, End, Indent, Outdent,
        StopEditing, Undo, Redo, DeleteWordBack, DeleteWordForward, DeleteLineBack,
        DeleteLineForward, WordLeft, WordRight, LineStart, LineEnd, PageStart, PageEnd,
        SelectLeft, SelectRight, SelectUp, SelectDown, SelectWordLeft, SelectWordRight,
        SelectLineStart, SelectLineEnd, SelectPageStart, SelectPageEnd, SelectAll, Copy, CopyMarkdown, Cut, Paste,
        Bold, Italic, Strike, InlineCode, EditLink, Heading1, Heading2, Heading3, Heading4, Heading5,
        Heading6, BodyText, BulletList, NumberedList, TaskList, ToggleDone, QuoteLine, CodeBlockLine
    ]
);

const CONTEXT: &str = "LiveView";

pub fn init(cx: &mut App) {
    let primary = |mac: &'static str, other: &'static str| if cfg!(target_os = "macos") { mac } else { other };
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("enter", Enter, Some(CONTEXT)),
        KeyBinding::new("shift-enter", SoftBreak, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("up", Up, Some(CONTEXT)),
        KeyBinding::new("down", Down, Some(CONTEXT)),
        KeyBinding::new("home", Home, Some(CONTEXT)),
        KeyBinding::new("end", End, Some(CONTEXT)),
        KeyBinding::new("tab", Indent, Some(CONTEXT)),
        KeyBinding::new("shift-tab", Outdent, Some(CONTEXT)),
        KeyBinding::new("escape", StopEditing, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-z", "ctrl-z"), Undo, Some(CONTEXT)),
        // The system's editing keys: ⌥ for words and ⌘ for lines on macOS,
        // Ctrl for words elsewhere.
        KeyBinding::new(primary("alt-backspace", "ctrl-backspace"), DeleteWordBack, Some(CONTEXT)),
        KeyBinding::new(primary("alt-delete", "ctrl-delete"), DeleteWordForward, Some(CONTEXT)),
        KeyBinding::new(primary("alt-left", "ctrl-left"), WordLeft, Some(CONTEXT)),
        KeyBinding::new(primary("alt-right", "ctrl-right"), WordRight, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-up", "ctrl-home"), PageStart, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-down", "ctrl-end"), PageEnd, Some(CONTEXT)),
        // Shift extends the selection with any of the moves.
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(CONTEXT)),
        KeyBinding::new(primary("alt-shift-left", "ctrl-shift-left"), SelectWordLeft, Some(CONTEXT)),
        KeyBinding::new(primary("alt-shift-right", "ctrl-shift-right"), SelectWordRight, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-left", "shift-home"), SelectLineStart, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-right", "shift-end"), SelectLineEnd, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-up", "ctrl-shift-home"), SelectPageStart, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-down", "ctrl-shift-end"), SelectPageEnd, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-a", "ctrl-a"), SelectAll, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-c", "ctrl-c"), Copy, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-c", "ctrl-shift-c"), CopyMarkdown, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-x", "ctrl-x"), Cut, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-v", "ctrl-v"), Paste, Some(CONTEXT)),
        // The editor's formatting keys. Inline code is ⌘E here, as in Notion:
        // ⌘⇧C copies Markdown.
        KeyBinding::new(primary("cmd-b", "ctrl-b"), Bold, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-i", "ctrl-i"), Italic, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-x", "ctrl-shift-x"), Strike, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-e", "ctrl-e"), InlineCode, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-k", "ctrl-k"), EditLink, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-1", "ctrl-1"), Heading1, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-2", "ctrl-2"), Heading2, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-3", "ctrl-3"), Heading3, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-4", "ctrl-4"), Heading4, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-5", "ctrl-5"), Heading5, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-6", "ctrl-6"), Heading6, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-0", "ctrl-0"), BodyText, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-8", "ctrl-shift-8"), BulletList, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-7", "ctrl-shift-7"), NumberedList, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-l", "ctrl-shift-l"), TaskList, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-enter", "ctrl-enter"), ToggleDone, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-.", "ctrl-shift-."), QuoteLine, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-alt-c", "ctrl-alt-c"), CodeBlockLine, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-z", "ctrl-y"), Redo, Some(CONTEXT)),
    ]);
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-backspace", DeleteLineBack, Some(CONTEXT)),
            KeyBinding::new("cmd-delete", DeleteLineForward, Some(CONTEXT)),
            KeyBinding::new("cmd-left", LineStart, Some(CONTEXT)),
            KeyBinding::new("cmd-right", LineEnd, Some(CONTEXT)),
            // The Cocoa emacs keys every macOS text field has.
            KeyBinding::new("ctrl-a", LineStart, Some(CONTEXT)),
            KeyBinding::new("ctrl-e", LineEnd, Some(CONTEXT)),
            KeyBinding::new("ctrl-h", Backspace, Some(CONTEXT)),
            KeyBinding::new("ctrl-d", Delete, Some(CONTEXT)),
            KeyBinding::new("ctrl-k", DeleteLineForward, Some(CONTEXT)),
        ]);
    }
}

/// What ⌘C puts on the clipboard beside the plain text, before the
/// Markdown, so a paste back into a note keeps the formatting.
const MARKDOWN_COPY: &str = "enso-markdown:";

/// The link card: what's edited, and where it shows (under the link).
struct LinkCard {
    /// The link's Markdown (or, for a new link, the selected text's).
    source: Range<usize>,
    wiki: bool,
    /// Where the link goes now, for Open.
    url: String,
    text_input: Entity<InputState>,
    url_input: Entity<InputState>,
    /// Where the card shows: under the link.
    at: Point<Pixels>,
    /// Shown by hovering (it goes when the pointer leaves), not by ⌘K.
    hover: bool,
    _subscriptions: Vec<Subscription>,
}

/// Edits closer together than this undo as one.
const UNDO_GROUP_MS: u128 = 800;
/// How far each level of a list is indented on the page.
const LIST_INDENT: f32 = 24.;

/// Where the caret is typing: in a line of text, or in a new paragraph
/// started between blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Spot {
    Leaf(usize),
    Gap(usize),
}

pub struct LiveView {
    editor: Entity<EditorState>,
    owner: WeakEntity<Enso>,
    editor_id: EntityId,
    base: Option<PathBuf>,
    pub(crate) focus: FocusHandle,
    scroll: ScrollHandle,
    source: String,
    page: Page,
    /// Where the caret is, in the Markdown; `None` while only reading.
    caret: Option<usize>,
    /// The selection's other end, when text is selected (the caret is the
    /// end that moves), in the Markdown.
    anchor: Option<usize>,
    /// A drag selecting text is under way.
    dragging: bool,
    /// The card for a link: its text and address, to edit.
    link_card: Option<LinkCard>,
    /// A link the pointer rests on, and the wait before its card shows.
    hovering: Option<(Range<usize>, Task<()>)>,
    /// Where each line of text was laid out last frame, by its key.
    geometry: Rc<RefCell<HashMap<u64, LeafGeometry>>>,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
    last_edit: Option<Instant>,
    /// Whether the page had the keyboard last frame: leaving it (for the
    /// sidebar, Split's editor, anywhere) ends editing.
    was_focused: bool,
    /// Editing set aside while something else had the keyboard (the insert
    /// menu, the link card): the caret and selection, and the note's
    /// revision then. When the page gets the keyboard back without a click,
    /// they come back as they were if the note is unchanged (Escape), else
    /// editing resumes at the editor's caret, after what was put in.
    parked: Option<(usize, Option<usize>, u64)>,
    /// Bumped whenever the note's text changes.
    revision: u64,
}

impl LiveView {
    pub fn new(
        editor: Entity<EditorState>,
        owner: WeakEntity<Enso>,
        base: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor_id = editor.entity_id();
        Self {
            editor,
            owner,
            editor_id,
            base,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            source: String::new(),
            page: Page::default(),
            caret: None,
            anchor: None,
            dragging: false,
            link_card: None,
            hovering: None,
            geometry: Rc::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            was_focused: false,
            parked: None,
            revision: 0,
        }
    }

    /// Pick up the note's text when it changed elsewhere (the insert menu,
    /// the disk, a ticked task). While typing, the caret follows the editor's,
    /// which a change made there (an insert) leaves after what it put in.
    fn refresh(&mut self, cx: &App) {
        let value = self.editor.read(cx).value();
        if value.as_ref() != self.source.as_str() {
            self.source = value.to_string();
            self.revision += 1;
            self.page = Page::read(&self.source);
            if self.caret.is_some() {
                let at = self.editor.read(cx).selected_range().end;
                self.caret = Some(floor_boundary(&self.source, at.min(self.source.len())));
            }
        }
    }

    fn spot(&self) -> Option<Spot> {
        let caret = self.caret?;
        match self.page.leaf_at(caret) {
            Some(leaf) => Some(Spot::Leaf(leaf)),
            // Between blocks, or on a blank line (one left in a list when it
            // ended): a new paragraph starts there.
            None if !self.page.in_block(caret) || on_blank_line(&self.source, caret) => Some(Spot::Gap(caret)),
            None => None,
        }
    }

    fn active_flow(&self) -> Option<Flow> {
        Some(match self.spot()? {
            Spot::Leaf(i) => self.page.leaves[i].flow.clone(),
            Spot::Gap(at) => Flow::empty(at),
        })
    }

    /// The caret within the edited line's drawn text.
    pub(crate) fn caret_in_active(&self) -> Option<usize> {
        Some(self.active_flow()?.shown_for(self.caret?))
    }

    fn shown_text(&self) -> Option<String> {
        Some(self.active_flow()?.text)
    }

    /// The note offset of drawn position `index` in the edited line.
    fn source_at(&self, index: usize) -> Option<usize> {
        Some(self.active_flow()?.source_for(index))
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn link_card_for_test(&self, cx: &App) -> Option<(String, String)> {
        self.link_card.as_ref().map(|c| (c.text_input.read(cx).value().to_string(), c.url_input.read(cx).value().to_string()))
    }

    /// A point on the first link's letters, and one on plain text, from the
    /// last layout.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn link_points_for_test(&self) -> Option<(Point<Pixels>, Point<Pixels>)> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            let g = geometry.get(&leaf_key(i))?;
            let at = (0..leaf.flow.text.len()).find(|x| leaf.flow.link_under(*x).is_some())?;
            let line = g.line_height() / 2.;
            let on = g.position_for_index(at + 1);
            let off = g.position_for_index(0);
            Some((point(on.x, on.y + line), point(off.x + px(1.), off.y + line)))
        })
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn caret_for_test(&self) -> Option<usize> {
        self.caret
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn set_caret_for_test(&mut self, caret: usize) {
        self.caret = Some(caret);
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn toggle_task_for_test(&mut self, leaf: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_task(leaf, window, cx);
    }

    /// The key the edited line's layout is reported under.
    pub(crate) fn active_key(&self) -> Option<u64> {
        Some(match self.spot()? {
            Spot::Leaf(i) => leaf_key(i),
            Spot::Gap(_) => GAP_LEAF,
        })
    }

    fn active_geometry(&self) -> Option<LeafGeometry> {
        self.geometry.borrow().get(&self.active_key()?).cloned()
    }

    /// The selected Markdown, when something is selected.
    fn selection(&self) -> Option<Range<usize>> {
        let (anchor, caret) = (self.anchor?, self.caret?);
        (anchor != caret).then(|| anchor.min(caret)..anchor.max(caret))
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    /// The note offset under `at` in line `leaf`, from its last layout.
    fn source_at_point(&self, leaf: usize, at: Point<Pixels>) -> Option<usize> {
        let index = self.geometry.borrow().get(&leaf_key(leaf))?.index_for_position(at);
        Some(self.page.leaves.get(leaf)?.flow.source_for(index))
    }

    /// The line nearest `at` (a drag past a line's end, or between lines).
    fn leaf_near(&self, at: Point<Pixels>) -> Option<usize> {
        let geometry = self.geometry.borrow();
        (0..self.page.leaves.len())
            .filter_map(|i| geometry.get(&leaf_key(i)).map(|g| (i, g.bounds())))
            .min_by_key(|(_, b)| {
                let dy = if at.y < b.top() { b.top() - at.y } else if at.y > b.bottom() { at.y - b.bottom() } else { px(0.) };
                (f32::from(dy) * 100.) as i64
            })
            .map(|(i, _)| i)
    }

    /// A press on line `leaf`: the caret goes there at once (one click), a
    /// word is selected (two) or the line (three); with Shift the selection
    /// grows to it. Dragging from there selects.
    fn mouse_down(&mut self, leaf: usize, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.read(cx).value().as_ref() != self.source.as_str() || !self.editor.read(cx).is_editable() {
            self.refresh(cx);
            cx.notify();
            return;
        }
        self.parked = None;
        window.focus(&self.focus, cx);
        let at = self.source_at_point(leaf, event.position).unwrap_or(self.page.leaves[leaf].span.start);
        let flow = self.page.leaves[leaf].flow.clone();
        match event.click_count {
            2 => {
                let shown = flow.shown_for(at);
                let end = flow.word_end(shown);
                self.anchor = Some(flow.source_for(flow.word_start(end)));
                self.caret = Some(flow.source_for(end));
            }
            3 => {
                self.anchor = Some(flow.source_for(0));
                self.caret = Some(flow.source_for(flow.text.len()));
            }
            _ if event.modifiers.shift && self.caret.is_some() => {
                self.anchor = self.anchor.or(self.caret);
                self.caret = Some(at);
            }
            _ => {
                self.anchor = Some(at);
                self.caret = Some(at);
            }
        }
        self.dragging = event.click_count == 1;
        // (A click on a link opens it: the text's own link handling. Its card
        // shows on hover, or with ⌘K.)
        self.link_card = None;
        cx.notify();
    }

    /// A press in a block of several lines (a quote): on the line nearest it.
    fn mouse_down_near(&mut self, leaves: &[usize], event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let leaf = {
            let geometry = self.geometry.borrow();
            leaves
                .iter()
                .filter_map(|i| geometry.get(&leaf_key(*i)).map(|g| (*i, g.bounds())))
                .min_by_key(|(_, b)| {
                    let at = event.position;
                    let dy = if at.y < b.top() { b.top() - at.y } else if at.y > b.bottom() { at.y - b.bottom() } else { px(0.) };
                    let dx = if at.x < b.left() { b.left() - at.x } else if at.x > b.right() { at.x - b.right() } else { px(0.) };
                    // Rows first, then the nearest cell across.
                    (f32::from(dy) * 100_000. + f32::from(dx) * 100.) as i64
                })
                .map(|(i, _)| i)
        };
        if let Some(leaf) = leaf.or(leaves.first().copied()) {
            self.mouse_down(leaf, event, window, cx);
        }
    }

    /// The line and link under the pointer, if it's on a link's letters.
    fn link_under(&self, at: Point<Pixels>) -> Option<(usize, LinkAt)> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            let g = geometry.get(&leaf_key(i))?;
            if !g.bounds().contains(&at) {
                return None;
            }
            leaf.flow.link_under(g.index_for_position(at)).map(|link| (i, link))
        })
    }

    /// The pointer resting on a link for a moment (0.8 s) shows its card; leaving
    /// the link (and not onto the card) puts a hover card away.
    fn hover(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        match self.link_under(at) {
            Some((leaf, link)) => {
                let shown = self.link_card.as_ref().is_some_and(|c| c.source == link.source);
                let waiting = self.hovering.as_ref().is_some_and(|(r, _)| *r == link.source);
                if shown || waiting {
                    return;
                }
                let source = link.source.clone();
                let task = cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(800)).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.hovering = None;
                        this.open_link_card(link, leaf, false, window, cx);
                    });
                });
                self.hovering = Some((source, task));
            }
            None => {
                self.hovering = None;
                if let Some(card) = &self.link_card {
                    let typing = card.text_input.focus_handle(cx).is_focused(window) || card.url_input.focus_handle(cx).is_focused(window);
                    if card.hover && !typing {
                        self.link_card = None;
                        cx.notify();
                    }
                }
            }
        }
    }

    fn mouse_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging || event.pressed_button != Some(MouseButton::Left) {
            self.hover(event.position, window, cx);
            return;
        }
        if let Some(at) = self.leaf_near(event.position).and_then(|leaf| self.source_at_point(leaf, event.position))
            && self.caret != Some(at)
        {
            self.caret = Some(at);
            cx.notify();
        }
    }

    /// A click on a block typed into only through its own controls (a table,
    /// an image): editing stops.
    fn click_other(&mut self, cx: &mut Context<Self>) {
        if self.editor.read(cx).value().as_ref() != self.source.as_str() {
            self.refresh(cx);
        }
        self.anchor = None;
        if self.caret.take().is_some() {
            cx.notify();
        }
    }

    /// Remove the selection from the note, leaving the caret where it began.
    /// Within one line its marks stay whole; across lines, the first line's
    /// start and the last line's end join.
    fn delete_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.anchor = None;
        let first = self.page.leaf_at(selection.start);
        let last = self.page.leaf_at(selection.end);
        let mut ranges: Vec<Range<usize>> = match (first, last) {
            (Some(a), Some(b)) if a == b => {
                let flow = &self.page.leaves[a].flow;
                flow.delete_shown(flow.shown_for(selection.start)..flow.shown_for(selection.end))
            }
            (Some(a), Some(b)) => {
                let (fa, fb) = (&self.page.leaves[a].flow, &self.page.leaves[b].flow);
                let mut r = fb.delete_shown(0..fb.shown_for(selection.end));
                r.push(fa.source_for(fa.text.len())..fb.source_for(0));
                r.extend(fa.delete_shown(fa.shown_for(selection.start)..fa.text.len()));
                r
            }
            _ => vec![selection.clone()],
        };
        ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
        for range in ranges.into_iter().filter(|r| !r.is_empty()) {
            self.edit(range, "", window, cx);
        }
        true
    }

    /// Run a move; with `extend` the selection grows with it, else it ends.
    fn moving(&mut self, extend: bool, cx: &mut Context<Self>, step: impl FnOnce(&mut Self, &mut Context<Self>)) {
        if extend {
            self.anchor = self.anchor.or(self.caret);
        } else {
            self.anchor = None;
        }
        step(self, cx);
    }

    /// Plain ← or → with a selection: the caret goes to that side of it.
    fn collapse(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.selection() else { return false };
        self.anchor = None;
        self.caret = Some(if forward { selection.end } else { selection.start });
        cx.notify();
        true
    }

    /// ⌘A, as in Notion: first the line the caret is in (a checkbox's text,
    /// a paragraph, the code); pressed again, the whole page.
    fn select_all(&mut self, cx: &mut Context<Self>) {
        if let Some(Spot::Leaf(i)) = self.spot() {
            let flow = &self.page.leaves[i].flow;
            let line = flow.source_for(0)..flow.source_for(flow.text.len());
            if !line.is_empty() && self.selection() != Some(line.clone()) {
                self.anchor = Some(line.start);
                self.caret = Some(line.end);
                cx.notify();
                return;
            }
        }
        let (Some(first), Some(last)) = (self.page.leaves.first(), self.page.leaves.last()) else { return };
        self.anchor = Some(first.flow.source_for(0));
        self.caret = Some(last.flow.source_for(last.flow.text.len()));
        cx.notify();
    }

    /// Each line the selection covers, with the part of it selected.
    fn selected_parts(&self) -> Vec<(usize, Range<usize>)> {
        let Some(selection) = self.selection() else { return Vec::new() };
        self.page
            .leaves
            .iter()
            .enumerate()
            .filter_map(|(i, leaf)| {
                let (start, end) = (selection.start.max(leaf.span.start), selection.end.min(leaf.span.end));
                if start > end {
                    return None;
                }
                let shown = leaf.flow.shown_for(start)..leaf.flow.shown_for(end);
                (!shown.is_empty()).then_some((i, shown))
            })
            .collect()
    }

    /// The selected text as read: no Markdown, a line break between lines.
    fn selected_text(&self) -> String {
        self.selected_parts()
            .into_iter()
            .map(|(i, shown)| self.page.leaves[i].flow.text[shown].to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The selected Markdown, marks whole at its ends.
    fn selected_markdown(&self) -> String {
        let parts = self.selected_parts();
        let (Some((a, first)), Some((b, last))) = (parts.first(), parts.last()) else { return String::new() };
        let start = self.page.leaves[*a].flow.source_span(first.clone()).start;
        let end = self.page.leaves[*b].flow.source_span(last.clone()).end;
        self.source[start..end.max(start)].to_string()
    }

    /// ⌘C: the plain text (the Markdown rides along unseen, so a paste here
    /// keeps the formatting). ⌘⇧C: the Markdown itself.
    fn copy(&mut self, markdown: bool, cx: &mut Context<Self>) -> bool {
        if self.selection().is_none() {
            return false;
        }
        let source = self.selected_markdown();
        let item = if markdown {
            ClipboardItem::new_string(source)
        } else {
            ClipboardItem {
                entries: vec![ClipboardEntry::String(
                    ClipboardString::new(self.selected_text()).with_json_metadata(format!("{MARKDOWN_COPY}{source}")),
                )],
            }
        };
        cx.write_to_clipboard(item);
        true
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            cx.propagate();
            return;
        };
        // Copied here: its Markdown, formatting and all.
        let copied = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(s) => s
                .metadata_json::<String>()
                .and_then(|m| m.strip_prefix(MARKDOWN_COPY).map(str::to_string)),
            _ => None,
        });
        let formatted = copied.is_some();
        let Some(text) = copied.or_else(|| item.text()) else {
            cx.propagate();
            return;
        };
        self.delete_selection(window, cx);
        let Some(caret) = self.caret else { return };
        // Into a table cell: one line, its `|`s as text.
        let in_cell = matches!(self.spot(), Some(Spot::Leaf(i)) if matches!(self.page.leaves[i].kind, Kind::Cell { .. }));
        let text = if in_cell {
            text.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
        } else {
            text
        };
        // Formatted text goes outside the marks the caret is at the edge of.
        let at = match self.active_flow() {
            Some(flow) if formatted => flow.source_outside_marks(flow.shown_for(caret)),
            _ => caret,
        };
        self.edit(at..at, &text, window, cx);
    }

    /// Replace `range` of the Markdown with `text`, through the note's editor
    /// (which saves it), and put the caret after it.
    fn edit(&mut self, range: Range<usize>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.read(cx).is_editable() {
            return;
        }
        let now = Instant::now();
        if self.last_edit.is_none_or(|at| now.duration_since(at).as_millis() > UNDO_GROUP_MS) {
            self.undo.push((self.source.clone(), self.caret.unwrap_or(range.start)));
        }
        self.last_edit = Some(now);
        self.redo.clear();
        let text = text.to_string();
        self.editor.update(cx, |editor, cx| {
            editor.set_selected_range(range.clone(), cx);
            editor.replace(text.clone(), window, cx);
        });
        self.refresh(cx);
        self.caret = Some(range.start + text.len());
        cx.notify();
    }

    /// An edit that leaves the caret where it was in the text (ticking a box,
    /// indenting a line).
    fn edit_keeping_caret(&mut self, range: Range<usize>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.caret;
        self.edit(range.clone(), text, window, cx);
        self.caret = caret.map(|c| {
            if c >= range.end {
                c + text.len() - range.len()
            } else if c > range.start {
                range.start + text.len()
            } else {
                c
            }
        });
        cx.notify();
    }

    fn restore(&mut self, to: (String, usize), window: &mut Window, cx: &mut Context<Self>) {
        let (text, caret) = to;
        self.editor.update(cx, |editor, cx| editor.set_value(text.as_str(), window, cx));
        self.refresh(cx);
        self.caret = Some(caret.min(self.source.len()));
        self.last_edit = None;
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.undo.pop() {
            self.redo.push((self.source.clone(), self.caret.unwrap_or(0)));
            self.restore(state, window, cx);
        }
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.redo.pop() {
            self.undo.push((self.source.clone(), self.caret.unwrap_or(0)));
            self.restore(state, window, cx);
        }
    }

    /// Tick or untick a task.
    fn toggle_task(&mut self, leaf: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Kind::Item(item)) = self.page.leaves.get(leaf).map(|l| l.kind.clone())
            && let Some((range, done)) = item.checkbox
        {
            self.edit_keeping_caret(range, if done { "[ ]" } else { "[x]" }, window, cx);
        }
    }

    /// Whether the caret's line has no text: an empty checkbox, heading,
    /// code block, quote line or new paragraph.
    fn spot_is_empty(&self, spot: Spot) -> bool {
        match spot {
            Spot::Gap(_) => true,
            Spot::Leaf(i) => self.page.leaves[i].flow.text.trim().is_empty(),
        }
    }

    /// The Markdown of an empty block (with the line break that set it off),
    /// to remove it whole.
    fn empty_block_range(&self, spot: Spot) -> Option<Range<usize>> {
        let len = self.source.len();
        let line = |at: usize| {
            let start = self.source[..at].rfind('\n').map_or(0, |n| n + 1);
            let end = self.source[at..].find('\n').map_or(len, |n| at + n);
            start..end
        };
        // A range with the break before it (or, at the top, the one after).
        let with_break = |r: Range<usize>| {
            if r.start > 0 { r.start - 1..r.end } else { r.start..(r.end + 1).min(len) }
        };
        let range = match spot {
            Spot::Gap(at) => {
                let blank = line(at);
                if quote_line(&self.source, at).is_some() || !blank.is_empty() {
                    with_break(blank)
                } else {
                    // Blank lines between blocks: those before the caret.
                    let from = self.source[..at].trim_end().len();
                    if from == at { return None; }
                    from..at
                }
            }
            Spot::Leaf(i) => match &self.page.leaves[i].kind {
                Kind::Item(item) => with_break(line(item.line_start)),
                Kind::Code { .. } | Kind::Text { .. } => {
                    let block = self.page.blocks.iter().find(|(r, _)| r.start <= self.page.leaves[i].span.start && self.page.leaves[i].span.start <= r.end)?.0.clone();
                    // Whole lines, so nothing of it (a trailing space) is left.
                    with_break(line(block.start).start..line(block.end.max(self.page.leaves[i].span.end)).end)
                }
                Kind::Quote => with_break(line(self.page.leaves[i].span.start)),
                // A cell isn't removed: the table keeps its shape.
                Kind::Cell { .. } => return None,
            },
        };
        (!range.is_empty()).then_some(range)
    }

    /// Remove the caret's empty block. Going back, the caret ends at the end
    /// of the line before; going forward, at the start of the line after.
    fn remove_empty_block(&mut self, spot: Spot, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(range) = self.empty_block_range(spot) else { return };
        let before = match spot {
            Spot::Leaf(i) => i.checked_sub(1),
            Spot::Gap(at) => self.page.leaves.iter().rposition(|l| l.span.end <= at),
        }
        .map(|p| {
            let flow = &self.page.leaves[p].flow;
            flow.source_for(flow.text.len())
        });
        self.edit(range.clone(), "", window, cx);
        let after = self.page.leaves.iter().find(|l| l.span.start >= range.start).map(|l| l.flow.source_for(0));
        let to = if forward { after.or(before) } else { before.or(after) };
        self.caret = Some(to.unwrap_or(range.start).min(self.source.len()));
        cx.notify();
    }

    /// Backspace at a line's start. An empty block goes. Otherwise a heading
    /// becomes a paragraph; a quote's paragraph leaves the quote; a list item
    /// joins the one above (the first loses its marker); a paragraph joins
    /// the line above.
    fn backspace_at_start(&mut self, spot: Spot, window: &mut Window, cx: &mut Context<Self>) {
        if self.spot_is_empty(spot) {
            self.remove_empty_block(spot, false, window, cx);
            return;
        }
        let Spot::Leaf(i) = spot else { return };
        let leaf = self.page.leaves[i].clone();
        let start = leaf.span.start;
        match &leaf.kind {
            // Code keeps its first line; a cell keeps its place in the table.
            Kind::Code { .. } | Kind::Cell { .. } => return,
            Kind::Quote => {
                let line_start = self.source[..start].rfind('\n').map_or(0, |n| n + 1);
                if line_start < start {
                    self.edit(line_start..start, "", window, cx);
                    self.caret = Some(line_start);
                }
                return;
            }
            Kind::Text { block } if leaf.flow.heading.is_some() => {
                let text_start = leaf.flow.source_for(0);
                self.edit(block.start..text_start, "", window, cx);
                self.caret = Some(block.start);
                return;
            }
            _ => {}
        }
        // The line before this one (never this one itself).
        let previous = i.checked_sub(1).map(|p| self.page.leaves[p].clone());
        if let Kind::Item(item) = &leaf.kind {
            let joinable = previous.as_ref().is_some_and(|p| {
                matches!(p.kind, Kind::Item(_))
                    && p.span.end <= item.prefix.start
                    && self.source[p.span.end..item.prefix.start].trim().is_empty()
            });
            if !joinable {
                // The first item: its marker goes; the text stays.
                self.edit(item.line_start..item.prefix.end, "", window, cx);
                self.caret = Some(item.line_start);
                return;
            }
        }
        let Some(previous) = previous.filter(|p| p.span.end <= start) else { return };
        let gap = previous.span.end..start;
        // Only blank lines and markers between: join the two lines.
        let between = &self.source[gap.clone()];
        let only_markers = between.lines().all(|line| {
            let t = line.trim();
            t.is_empty() || t.starts_with(['-', '*', '+', '>']) || t.chars().next().is_some_and(|c| c.is_ascii_digit())
        });
        if only_markers && between.len() < 64 {
            self.edit(gap.clone(), "", window, cx);
            self.caret = Some(previous.span.end);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.delete_selection(window, cx) {
            return;
        }
        let (Some(caret), Some(spot), Some(flow)) = (self.caret, self.spot(), self.active_flow()) else { return };
        let at = flow.shown_for(caret);
        // An empty block (a space or two at most) goes whole.
        if at == 0 || self.spot_is_empty(spot) {
            self.backspace_at_start(spot, window, cx);
            return;
        }
        if let Some(gone) = flow.delete_before(at) {
            self.edit(gone, "", window, cx);
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.delete_selection(window, cx) {
            return;
        }
        if let Some(spot) = self.spot().filter(|s| self.spot_is_empty(*s)) {
            self.remove_empty_block(spot, true, window, cx);
            return;
        }
        let (Some(caret), Some(flow)) = (self.caret, self.active_flow()) else { return };
        if let Some(gone) = flow.delete_after(flow.shown_for(caret)) {
            self.edit(gone.clone(), "", window, cx);
            self.caret = Some(gone.start);
        }
    }

    /// Enter: a new paragraph, or in a list a new item like this one (on an
    /// empty item, the list ends and a paragraph starts). Marks around the
    /// caret close before the break and reopen after.
    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection(window, cx);
        if self.table_key("enter", window, cx) {
            return;
        }
        let (Some(caret), Some(spot), Some(flow)) = (self.caret, self.spot(), self.active_flow()) else { return };
        match spot {
            // In code, Enter is a new line of code.
            Spot::Leaf(i) if matches!(self.page.leaves[i].kind, Kind::Code { .. }) => {
                self.edit(caret..caret, "\n", window, cx);
                return;
            }
            // In a quote, a new paragraph in the quote.
            Spot::Leaf(i) if matches!(self.page.leaves[i].kind, Kind::Quote) => {
                let (at, text) = flow.split_at(flow.shown_for(caret));
                let text = text.replacen("\n\n", "\n>\n> ", 1);
                self.edit(at..at, &text, window, cx);
                return;
            }
            // On an empty quote line: out of the quote.
            Spot::Gap(at) if quote_line(&self.source, at).is_some() => {
                let line = quote_line(&self.source, at).unwrap_or(at..at);
                let from = line.start.saturating_sub(1);
                self.edit(from..line.end, "\n\n", window, cx);
                return;
            }
            _ => {}
        }
        if let Spot::Leaf(i) = spot
            && let Kind::Item(item) = &self.page.leaves[i].kind
        {
            if flow.text.is_empty() {
                // Out of the list: the empty item becomes a new paragraph.
                let line_end = self.source[item.line_start..].find('\n').map_or(self.source.len(), |n| item.line_start + n);
                let from = item.line_start.saturating_sub(1);
                self.edit(from..line_end, if item.line_start == 0 { "" } else { "\n\n" }, window, cx);
                return;
            }
            let (at, text) = flow.split_at(flow.shown_for(caret));
            let text = text.replacen("\n\n", &format!("\n{}", item.next_prefix), 1);
            self.edit(at..at, &text, window, cx);
            return;
        }
        if flow.text.is_empty() {
            self.edit(caret..caret, "\n\n", window, cx);
        } else {
            let (at, text) = flow.split_at(flow.shown_for(caret));
            self.edit(at..at, &text, window, cx);
        }
    }

    fn open_insert_menu(&mut self, caret: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let anchor = {
            match (self.active_geometry(), self.caret_in_active()) {
                (Some(g), Some(local)) => {
                    let caret = caret_bounds(&g, local);
                    point(caret.left(), caret.bottom() + px(6.))
                }
                _ => point(px(100.), px(120.)),
            }
        };
        self.owner
            .update(cx, |app, cx| app.open_live_insert_menu(caret, anchor, window, cx))
            .unwrap_or(false)
    }

    /// Delete drawn text `shown` of the edited line, keeping its Markdown
    /// whole (a mark that goes completely takes its delimiters along).
    fn delete_shown(&mut self, shown: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(flow) = self.active_flow() else { return };
        if shown.is_empty() {
            return;
        }
        for range in flow.delete_shown(shown) {
            self.edit(range, "", window, cx);
        }
    }

    /// Delete back to the word's start (⌥⌫), or to the line's (⌘⌫); at the
    /// line's very start, join it to the line above as Backspace does.
    fn delete_back(&mut self, line: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.delete_selection(window, cx) {
            return;
        }
        let (Some(caret), Some(spot), Some(flow)) = (self.caret, self.spot(), self.active_flow()) else { return };
        let at = flow.shown_for(caret);
        if at == 0 {
            self.backspace_at_start(spot, window, cx);
            return;
        }
        let from = if line { self.line_edge(false).unwrap_or(0).min(at) } else { flow.word_start(at) };
        // Already at the start of a wrapped line: take the character before.
        let from = if from == at { previous_boundary(&flow.text, at) } else { from };
        self.delete_shown(from..at, window, cx);
    }

    fn delete_forward(&mut self, line: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.delete_selection(window, cx) {
            return;
        }
        let (Some(caret), Some(flow)) = (self.caret, self.active_flow()) else { return };
        let at = flow.shown_for(caret);
        let to = if line { self.line_edge(true).unwrap_or(flow.text.len()).max(at) } else { flow.word_end(at) };
        self.delete_shown(at..to, window, cx);
        self.caret = Some(flow.source_for(at).min(self.source.len()));
        cx.notify();
    }

    /// The start (or end) of the caret's visual line, as drawn.
    fn line_edge(&self, end: bool) -> Option<usize> {
        let local = self.caret_in_active()?;
        let geometry = self.active_geometry()?;
        let at = geometry.position_for_index(local);
        let bounds = geometry.bounds();
        let y = at.y + geometry.line_height() / 2.;
        let x = if end { bounds.right() + px(1.) } else { bounds.left() - px(1.) };
        Some(geometry.index_for_position(point(x, y)))
    }

    fn move_to_shown(&mut self, shown: usize, cx: &mut Context<Self>) {
        if let Some(to) = self.source_at(shown) {
            self.caret = Some(to);
            cx.notify();
        }
    }

    /// ⌥← / ⌥→: a word at a time, into the line before or after at its edge.
    fn word_move(&mut self, forward: bool, cx: &mut Context<Self>) {
        let (Some(caret), Some(flow)) = (self.caret, self.active_flow()) else { return };
        let at = flow.shown_for(caret);
        if (forward && at >= flow.text.len()) || (!forward && at == 0) {
            self.horizontal(forward, cx);
            return;
        }
        self.move_to_shown(if forward { flow.word_end(at) } else { flow.word_start(at) }, cx);
    }

    fn line_move(&mut self, end: bool, cx: &mut Context<Self>) {
        let len = self.active_flow().map_or(0, |f| f.text.len());
        let to = self.line_edge(end).unwrap_or(if end { len } else { 0 });
        self.move_to_shown(to, cx);
    }

    /// ⌘↑ / ⌘↓: the start of the first line, or the end of the last.
    fn page_edge(&mut self, end: bool, cx: &mut Context<Self>) {
        let leaf = if end { self.page.leaves.last() } else { self.page.leaves.first() };
        if let Some(leaf) = leaf {
            self.caret = Some(if end { leaf.flow.source_for(leaf.flow.text.len()) } else { leaf.flow.source_for(0) });
            cx.notify();
        }
    }

    fn soft_break(&mut self, _: &SoftBreak, window: &mut Window, cx: &mut Context<Self>) {
        let Some(caret) = self.caret else { return };
        self.edit(caret..caret, "  \n", window, cx);
    }

    /// Tab indents a list item under the one above; Shift+Tab brings it back.
    fn indent(&mut self, outdent: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.table_key(if outdent { "shift-tab" } else { "tab" }, window, cx) {
            return;
        }
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        if let (Kind::Code { .. }, Some(caret), false) = (&self.page.leaves[i].kind, self.caret, outdent) {
            self.delete_selection(window, cx);
            self.edit(caret..caret, "    ", window, cx);
            return;
        }
        let Kind::Item(item) = self.page.leaves[i].kind.clone() else { return };
        if outdent {
            let leading = self.source[item.line_start..].chars().take_while(|c| *c == ' ').count();
            let width = leading.min(item.indent.max(2));
            if width > 0 {
                self.edit_keeping_caret(item.line_start..item.line_start + width, "", window, cx);
            }
            return;
        }
        // Only under an item above.
        let above = self.page.leaves[..i].iter().rev().find_map(|l| match &l.kind {
            Kind::Item(above) => Some(above.clone()),
            _ => None,
        });
        if let Some(above) = above.filter(|a| self.source[a.range.end.min(item.line_start)..item.line_start].trim().is_empty()) {
            let spaces = " ".repeat(above.indent);
            self.edit_keeping_caret(item.line_start..item.line_start, &spaces, window, cx);
        }
    }

    /// Left and right move a character, and past a line's edge into the line
    /// before or after.
    fn horizontal(&mut self, forward: bool, cx: &mut Context<Self>) {
        let (Some(caret), Some(spot), Some(flow)) = (self.caret, self.spot(), self.active_flow()) else { return };
        let at = flow.shown_for(caret);
        let edge = if forward { at >= flow.text.len() } else { at == 0 };
        let to = if edge {
            self.neighbour(spot, forward).map(|l| {
                let leaf = &self.page.leaves[l];
                if forward { leaf.flow.source_for(0) } else { leaf.flow.source_for(leaf.flow.text.len()) }
            })
        } else {
            let to = if forward { next_boundary(&flow.text, at) } else { previous_boundary(&flow.text, at) };
            Some(flow.source_for(to))
        };
        if let Some(to) = to {
            self.caret = Some(to);
            cx.notify();
        }
    }

    /// The line after (or before) the caret's.
    fn neighbour(&self, spot: Spot, forward: bool) -> Option<usize> {
        let (start, end) = match spot {
            Spot::Leaf(i) => {
                return if forward {
                    (i + 1 < self.page.leaves.len()).then_some(i + 1)
                } else {
                    i.checked_sub(1)
                };
            }
            Spot::Gap(at) => (at, at),
        };
        if forward {
            self.page.leaves.iter().position(|l| l.span.start > end)
        } else {
            self.page.leaves.iter().rposition(|l| l.span.end < start)
        }
    }

    /// Up and down move a line within the text, and past its first or last
    /// line into the line above or below.
    fn vertical(&mut self, down: bool, cx: &mut Context<Self>) {
        let (Some(local), Some(spot)) = (self.caret_in_active(), self.spot()) else { return };
        if let Some(geometry) = self.active_geometry() {
            let at = geometry.position_for_index(local);
            let line = geometry.line_height();
            let target = point(at.x, if down { at.y + line * 1.5 } else { at.y - line * 0.5 });
            let bounds = geometry.bounds();
            if target.y >= bounds.top() && target.y <= bounds.bottom() {
                let index = geometry.index_for_position(target);
                if let Some(to) = self.source_at(index) {
                    self.caret = Some(to);
                    cx.notify();
                }
                return;
            }
        }
        if let Spot::Leaf(i) = spot
            && let Some(target) = self.table_vertical(i, down)
        {
            if let Some(l) = target {
                let flow = &self.page.leaves[l].flow;
                self.caret = Some(flow.source_for(local.min(flow.text.len())));
                cx.notify();
            }
            return;
        }
        if let Some(l) = self.neighbour(spot, down) {
            let flow = &self.page.leaves[l].flow;
            self.caret = Some(flow.source_for(local.min(flow.text.len())));
            cx.notify();
        }
    }

    /// ⌘B, ⌘I, ⌘⇧X, ⌘E: the mark on the selection (or the word at the
    /// caret) on or off, the selection kept.
    fn toggle_mark(&mut self, kind: Wrap, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(caret), Some(Spot::Leaf(i))) = (self.caret, self.spot()) else { return };
        if matches!(self.page.leaves[i].kind, Kind::Code { .. }) {
            return;
        }
        let flow = self.page.leaves[i].flow.clone();
        let selected = self.selection();
        let shown = match &selected {
            Some(sel) if self.page.leaf_at(sel.start) == Some(i) && self.page.leaf_at(sel.end) == Some(i) => {
                flow.shown_for(sel.start)..flow.shown_for(sel.end)
            }
            Some(_) => return,
            None => {
                let at = flow.shown_for(caret);
                let end = flow.word_end(at);
                flow.word_start(end)..end
            }
        };
        let edits = flow.toggle(kind, shown.clone());
        if edits.is_empty() {
            return;
        }
        let at = flow.shown_for(caret);
        for (range, text) in edits {
            self.edit(range, &text, window, cx);
        }
        // The same text stays selected (or the caret where it was).
        if let Some(flow) = self.page.leaves.get(i).map(|l| l.flow.clone()) {
            if selected.is_some() {
                if let Some(span) = flow.char_span(shown) {
                    self.anchor = Some(span.start);
                    self.caret = Some(span.end);
                }
            } else {
                self.anchor = None;
                self.caret = Some(flow.source_for(at));
            }
        }
        cx.notify();
    }

    /// The paragraph or heading line's prefix (`## `) and its range.
    fn text_prefix(&self) -> Option<(usize, Range<usize>)> {
        let Some(Spot::Leaf(i)) = self.spot() else { return None };
        let leaf = &self.page.leaves[i];
        match &leaf.kind {
            Kind::Text { block } => Some((i, block.start..leaf.flow.source_for(0).max(block.start))),
            _ => None,
        }
    }

    /// ⌘1…⌘6 make the line a heading of that level; ⌘0 plain text.
    fn set_heading(&mut self, level: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, prefix)) = self.text_prefix() else { return };
        let new = if level == 0 { String::new() } else { format!("{} ", "#".repeat(level)) };
        if self.source[prefix.clone()] != new {
            self.edit_keeping_caret(prefix, &new, window, cx);
        }
    }

    /// ⌘⇧8, ⌘⇧7, ⌘⇧L: the line becomes a bullet, numbered or checkbox
    /// item; pressed on an item of that kind, it's plain text again.
    fn set_list(&mut self, marker: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        match self.page.leaves[i].kind.clone() {
            Kind::Text { .. } => {
                if let Some((_, prefix)) = self.text_prefix() {
                    self.edit_keeping_caret(prefix, marker, window, cx);
                }
            }
            Kind::Item(item) => {
                let current = self.source[item.prefix.clone()].to_string();
                let same = match marker {
                    "- [ ] " => item.checkbox.is_some(),
                    "1. " => item.checkbox.is_none() && current.trim_start().starts_with(|c: char| c.is_ascii_digit()),
                    _ => item.checkbox.is_none() && !current.trim_start().starts_with(|c: char| c.is_ascii_digit()),
                };
                let new = if same { "" } else { marker };
                self.edit_keeping_caret(item.prefix.clone(), new, window, cx);
            }
            _ => {}
        }
    }

    /// ⌘↵: tick or untick the caret's checkbox.
    fn toggle_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Spot::Leaf(i)) = self.spot() {
            self.toggle_task(i, window, cx);
        }
    }

    /// ⌘⇧.: the paragraph becomes a quote, or the quote's line plain text.
    fn quote_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let start = self.page.leaves[i].span.start;
        match &self.page.leaves[i].kind {
            Kind::Text { .. } => {
                if let Some((_, prefix)) = self.text_prefix() {
                    self.edit_keeping_caret(prefix, "> ", window, cx);
                }
            }
            Kind::Quote => {
                let line_start = self.source[..start].rfind('\n').map_or(0, |n| n + 1);
                self.edit_keeping_caret(line_start..start, "", window, cx);
            }
            _ => {}
        }
    }

    /// ⌥⌘C: the paragraph becomes a code block holding its text.
    fn code_block_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { return };
        let Kind::Text { block } = self.page.leaves[i].kind.clone() else { return };
        let text = self.page.leaves[i].flow.text.clone();
        let code = format!("```\n{text}\n```");
        self.edit(block.clone(), &code, window, cx);
        self.caret = Some(block.start + 4 + text.len());
        cx.notify();
    }

    /// The link card for `link`, under its first letter.
    fn open_link_card(&mut self, link: LinkAt, leaf: usize, focus_url: bool, window: &mut Window, cx: &mut Context<Self>) {
        let at = {
            let geometry = self.geometry.borrow();
            let flow = &self.page.leaves[leaf].flow;
            geometry.get(&leaf_key(leaf)).map(|g| {
                let top = g.position_for_index(flow.shown_for(link.source.start));
                point(top.x, top.y + g.line_height() + px(4.))
            })
        }
        .unwrap_or(point(px(120.), px(120.)));
        let target = if link.wiki { link.url.trim_start_matches("enso-wiki:").to_string() } else { link.url.clone() };
        let target = urlencoding_decode(&target);
        let text_input = cx.new(|cx| InputState::new(window, cx).placeholder("Text").default_value(link.text.clone()));
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(if link.wiki { "Note" } else { "https://…" })
                .default_value(target)
        });
        let apply = |this: &mut Self, _: &Entity<InputState>, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_link(window, cx);
            }
        };
        let subscriptions = vec![
            cx.subscribe_in(&text_input, window, apply),
            cx.subscribe_in(&url_input, window, apply),
        ];
        if focus_url {
            url_input.update(cx, |input, cx| input.focus(window, cx));
        }
        self.link_card = Some(LinkCard {
            source: link.source,
            wiki: link.wiki,
            url: link.url,
            text_input,
            url_input,
            at,
            hover: !focus_url,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// Write the card's text and address back as the link's Markdown.
    fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.link_card.take() else { return };
        let text = card.text_input.read(cx).value().trim().to_string();
        let target = card.url_input.read(cx).value().trim().to_string();
        let markdown = if target.is_empty() {
            text.clone()
        } else if card.wiki {
            if text.is_empty() || text == target { format!("[[{target}]]") } else { format!("[[{target}|{text}]]") }
        } else {
            let label = if text.is_empty() { target.clone() } else { text.replace('[', "\\[").replace(']', "\\]") };
            let target = if target.contains(' ') { format!("<{target}>") } else { target };
            format!("[{label}]({target})")
        };
        self.focus_page(window, cx);
        self.edit(card.source, &markdown, window, cx);
    }

    /// Take the link off, keeping its text.
    fn remove_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.link_card.take() else { return };
        let text = card.text_input.read(cx).value().to_string();
        self.focus_page(window, cx);
        self.edit(card.source, &text, window, cx);
    }

    fn close_link_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.link_card.take().is_some() {
            self.focus_page(window, cx);
            cx.notify();
        }
    }

    fn focus_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    /// Open a link: a web address in the browser, a note in enso.
    fn open_link(&self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
            cx.open_url(url);
        } else {
            let (editor, url) = (self.editor_id, url.to_string());
            let _ = self.owner.update(cx, |app, cx| app.follow_note_link(editor, &url, window, cx));
        }
    }

    /// ⌘K: the card for the link at the caret, or a new link on the selection.
    fn edit_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(caret), Some(Spot::Leaf(i))) = (self.caret, self.spot()) else { return };
        let flow = self.page.leaves[i].flow.clone();
        if let Some(link) = flow.link_at(flow.shown_for(caret)) {
            self.open_link_card(link, i, true, window, cx);
            return;
        }
        let Some(selection) = self.selection() else { return };
        if self.page.leaf_at(selection.start) != Some(i) || self.page.leaf_at(selection.end) != Some(i) {
            return;
        }
        let shown = flow.shown_for(selection.start)..flow.shown_for(selection.end);
        let source = flow.source_span(shown.clone());
        let link = LinkAt { source, text: flow.text[shown].to_string(), url: String::new(), wiki: false };
        self.open_link_card(link, i, true, window, cx);
    }

    /// The table holding cell `leaf`: its cells row by row, and its end.
    fn table_of(&self, leaf: usize) -> Option<(Vec<Vec<usize>>, usize)> {
        self.page.blocks.iter().find_map(|(_, shape)| match shape {
            Shape::Table { rows, end, .. } if rows.iter().flatten().any(|i| *i == leaf) => Some((rows.clone(), *end)),
            _ => None,
        })
    }

    /// Put the caret at the start (or end) of cell `leaf`.
    fn go_to_cell(&mut self, leaf: usize, end: bool, cx: &mut Context<Self>) {
        let flow = &self.page.leaves[leaf].flow;
        self.anchor = None;
        self.caret = Some(if end { flow.source_for(flow.text.len()) } else { flow.source_for(0) });
        cx.notify();
    }

    /// Add an empty row at the table's end and go to its cell `col`.
    fn add_table_row(&mut self, rows: &[Vec<usize>], end: usize, col: usize, window: &mut Window, cx: &mut Context<Self>) {
        let width = rows.first().map_or(1, |r| r.len()).max(1);
        let row = format!("\n|{}", "  |".repeat(width));
        let new_row = rows.len();
        self.edit(end..end, &row, window, cx);
        let cell = self.page.leaves.iter().position(|l| matches!(l.kind, Kind::Cell { row, col: c } if row == new_row && c == col) && l.span.start > end);
        if let Some(cell) = cell {
            self.go_to_cell(cell, false, cx);
        }
    }

    /// In a table: Tab to the next cell (a new row after the last), Shift+Tab
    /// back, Enter to the cell below (a new row after the last). False when
    /// the caret isn't in a table.
    fn table_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(Spot::Leaf(i)) = self.spot() else { return false };
        let Kind::Cell { row, col } = self.page.leaves[i].kind else { return false };
        let Some((rows, end)) = self.table_of(i) else { return false };
        let order: Vec<usize> = rows.iter().flatten().copied().collect();
        let at = order.iter().position(|l| *l == i).unwrap_or(0);
        match key {
            "tab" => match order.get(at + 1) {
                Some(next) => self.go_to_cell(*next, false, cx),
                None => self.add_table_row(&rows, end, 0, window, cx),
            },
            "shift-tab" => {
                if let Some(previous) = at.checked_sub(1).map(|p| order[p]) {
                    self.go_to_cell(previous, false, cx);
                }
            }
            _ => match rows.get(row + 1).and_then(|r| r.get(col)) {
                Some(below) => self.go_to_cell(*below, false, cx),
                None => self.add_table_row(&rows, end, col, window, cx),
            },
        }
        true
    }

    /// ↑ / ↓ past a cell's first or last line: the cell above or below, or
    /// out of the table. `None` when the caret isn't in a cell.
    fn table_vertical(&self, leaf: usize, down: bool) -> Option<Option<usize>> {
        let Kind::Cell { row, col } = self.page.leaves[leaf].kind else { return None };
        let (rows, _) = self.table_of(leaf)?;
        let target = if down { rows.get(row + 1) } else { row.checked_sub(1).and_then(|r| rows.get(r)) };
        if let Some(cell) = target.and_then(|r| r.get(col)) {
            return Some(Some(*cell));
        }
        // Out of the table: the line after it, or before it.
        let first = *rows.first()?.first()?;
        let last = *rows.last()?.last()?;
        Some(if down { (last + 1 < self.page.leaves.len()).then_some(last + 1) } else { first.checked_sub(1) })
    }

    /// ↓ on the page's last line (out of a code block, say): a new paragraph
    /// after everything.
    fn past_the_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.source.len();
        if !self.page.in_block(end) {
            self.caret = Some(end);
            cx.notify();
        } else if self.editor.read(cx).is_editable()
            && matches!(self.page.blocks.last(), Some((_, Shape::Code(_) | Shape::Quote(_) | Shape::List(_) | Shape::Table { .. })))
        {
            self.edit(end..end, "\n\n", window, cx);
        }
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.source_at(0) {
            self.caret = Some(to);
            cx.notify();
        }
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let Some(len) = self.active_flow().map(|f| f.text.len()) else { return };
        if let Some(to) = self.source_at(len) {
            self.caret = Some(to);
            cx.notify();
        }
    }

    fn stop_editing(&mut self, _: &StopEditing, window: &mut Window, cx: &mut Context<Self>) {
        if self.link_card.is_some() {
            self.close_link_card(window, cx);
            return;
        }
        if self.caret.take().is_some() {
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    /// A line of text drawn from its flow, reporting its layout under `key`
    /// so clicks, the caret and the selection can find its letters.
    fn flow_node(flow: &Flow, key: u64) -> BlockNode {
        let mut paragraph = Paragraph::default();
        paragraph.push(InlineNode::new(flow.text.clone()).marks(flow.marks.clone()));
        let paragraph = paragraph.with_leaf_key(key);
        match flow.heading {
            Some(level) => BlockNode::Heading { level, children: paragraph, span: None },
            None => BlockNode::Paragraph(paragraph),
        }
    }
}

impl Render for LiveView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        if self.was_focused && !focused {
            self.parked = self.caret.take().map(|caret| (caret, self.anchor.take(), self.revision));
            self.dragging = false;
        }
        self.refresh(cx);
        if !self.was_focused && focused && self.caret.is_none()
            && let Some((caret, anchor, revision)) = self.parked.take()
        {
            if revision == self.revision {
                self.caret = Some(caret);
                self.anchor = anchor;
            } else {
                let at = self.editor.read(cx).selected_range().end;
                self.caret = Some(floor_boundary(&self.source, at.min(self.source.len())));
            }
        }
        self.was_focused = focused;
        let spot = self.spot();
        let reading = typography::current(cx);
        let render_context = {
            let (owner, editor) = (self.owner.clone(), self.editor_id);
            RenderContext::from_app(cx).with_body_font_size(px(reading.size as f32)).on_link_click(move |url, _, window, cx| {
                // Web addresses in the browser; notes (wiki links too) in enso.
                if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
                    cx.open_url(url);
                } else {
                    let url = url.to_string();
                    let _ = owner.update(cx, |app, cx| app.follow_note_link(editor, &url, window, cx));
                }
            })
        };
        let muted = cx.theme().muted_foreground;
        let weak = cx.entity().downgrade();
        let count = self.page.blocks.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut gap_placed = false;
        // A line of text: pressed, the caret goes there (and a drag selects).
        let line = |leaf: usize, first: bool, last: bool, rows_window: &mut Window, cx: &mut App| {
            let flow = &self.page.leaves[leaf].flow;
            let node = Self::flow_node(flow, leaf_key(leaf));
            let position = FlowPosition::at(&[&node], 0, last).first(first);
            let element = node.render_with_id(("live-line", leaf), position, &render_context, rows_window, cx);
            let weak = weak.clone();
            div()
                .id(("live-text", leaf))
                .w_full()
                .min_w_0()
                .cursor_text()
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    let _ = weak.update(cx, |view, cx| view.mouse_down(leaf, event, window, cx));
                })
                .child(element)
                .into_any_element()
        };
        let gap = |at: usize, first: bool, rows_window: &mut Window, cx: &mut App| {
            let node = Self::flow_node(&Flow::empty(at), GAP_LEAF);
            let position = FlowPosition::at(&[&node], 0, false).first(first);
            node.render_with_id(("live-new", at), position, &render_context, rows_window, cx)
        };
        for (b, (range, shape)) in self.page.blocks.clone().into_iter().enumerate() {
            // A new paragraph being written between blocks.
            if let Some(Spot::Gap(at)) = spot
                && !gap_placed
                && at < range.start
            {
                rows.push(gap(at, rows.is_empty(), window, cx));
                gap_placed = true;
            }
            let (first, last) = (b == 0, b + 1 == count);
            let element = match shape {
                Shape::Text(leaf) => line(leaf, first, last, window, cx),
                Shape::List(items) => {
                    let mut list = v_flex().w_full().gap_1().py_1();
                    for row in items {
                        // One text line tall, the marker centred in it, so it
                        // sits beside the item's first line at any size.
                        let line_height = px(reading.size as f32 * 1.618);
                        let slot = || div().h(line_height).min_w(px(18.)).flex_shrink_0().flex().items_center().text_color(muted);
                        let marker = match row.marker {
                            Marker::Bullet => slot().child("•").into_any_element(),
                            Marker::Number(n) => slot().child(format!("{n}.")).into_any_element(),
                            Marker::Task(done) => {
                                let weak = weak.clone();
                                let leaf = row.leaf;
                                slot()
                                    .child(Checkbox::new(("live-task", leaf)).checked(done).on_click(move |_, window, cx| {
                                        let _ = weak.update(cx, |view, cx| view.toggle_task(leaf, window, cx));
                                    }))
                                    .into_any_element()
                            }
                        };
                        list = list.child(
                            h_flex()
                                .w_full()
                                .items_start()
                                .gap_2()
                                .pl(px(LIST_INDENT * row.depth as f32))
                                .child(marker)
                                .child(line(row.leaf, true, true, window, cx)),
                        );
                    }
                    list.into_any_element()
                }
                Shape::Quote(leaves) => {
                    let children = leaves.iter().map(|i| Self::flow_node(&self.page.leaves[*i].flow, leaf_key(*i))).collect();
                    let node = BlockNode::Blockquote { children, span: None };
                    let position = FlowPosition::at(&[&node], 0, last).first(first);
                    let element = node.render_with_id(("live-quote", range.start), position, &render_context, window, cx);
                    let weak = weak.clone();
                    div()
                        .id(("live-quote-row", range.start))
                        .w_full()
                        .cursor_text()
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            let leaves = leaves.clone();
                            let _ = weak.update(cx, |view, cx| view.mouse_down_near(&leaves, event, window, cx));
                        })
                        .child(element)
                        .into_any_element()
                }
                Shape::Table { rows: cells, aligns, .. } => {
                    let paragraphs = cells
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|i| {
                                    let mut p = Paragraph::default();
                                    let flow = &self.page.leaves[*i].flow;
                                    p.push(InlineNode::new(flow.text.clone()).marks(flow.marks.clone()));
                                    p.with_leaf_key(leaf_key(*i))
                                })
                                .collect()
                        })
                        .collect();
                    let aligns = aligns
                        .iter()
                        .map(|a| match a {
                            Align::Left => ColumnAlign::Left,
                            Align::Center => ColumnAlign::Center,
                            Align::Right => ColumnAlign::Right,
                            Align::None => ColumnAlign::None,
                        })
                        .collect();
                    let node = BlockNode::Table(Table::new(paragraphs, aligns));
                    let position = FlowPosition::at(&[&node], 0, last).first(first);
                    let element = node.render_with_id(("live-table", range.start), position, &render_context, window, cx);
                    let leaves: Vec<usize> = cells.iter().flatten().copied().collect();
                    let weak = weak.clone();
                    div()
                        .id(("live-table-row", range.start))
                        .w_full()
                        .cursor_text()
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            let leaves = leaves.clone();
                            let _ = weak.update(cx, |view, cx| view.mouse_down_near(&leaves, event, window, cx));
                        })
                        .child(element)
                        .into_any_element()
                }
                Shape::Code(leaf) => {
                    let kind = &self.page.leaves[leaf].kind;
                    let lang = match kind {
                        Kind::Code { lang } => lang.clone(),
                        _ => None,
                    };
                    let code = CodeBlock::from_code(self.page.leaves[leaf].flow.text.clone(), lang).with_leaf_key(leaf_key(leaf));
                    let node = BlockNode::CodeBlock(code);
                    let position = FlowPosition::at(&[&node], 0, last).first(first);
                    let element = node.render_with_id(("live-code", range.start), position, &render_context, window, cx);
                    let weak = weak.clone();
                    div()
                        .id(("live-code-row", range.start))
                        .w_full()
                        .cursor_text()
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            let _ = weak.update(cx, |view, cx| view.mouse_down(leaf, event, window, cx));
                        })
                        .child(element)
                        .into_any_element()
                }
                Shape::Other => {
                    let view = TextView::markdown(("live-view", range.start), self.source[range.clone()].to_string())
                        .selectable(false)
                        .w_full();
                    let view = typography::style(view, cx);
                    let base = self.base.clone();
                    let image_base = base.clone();
                    let weak = weak.clone();
                    div()
                        .id(("live-block", range.start))
                        .w_full()
                        .on_click(move |event, window, cx| {
                            let at = event.position();
                            let _ = (at, window);
                            let _ = weak.update(cx, |view, cx| view.click_other(cx));
                        })
                        .child(
                            rich::decorate(view, self.owner.clone(), self.editor_id, base, range.start, 0).image_source(
                                move |uri| {
                                    let source = uri.to_string();
                                    if !source.contains(':') {
                                        if let Some(base) = &image_base {
                                            return ImageSource::from(base.join(source));
                                        }
                                    }
                                    ImageSource::from(uri.clone())
                                },
                            ),
                        )
                        .into_any_element()
                }
            };
            rows.push(element);
        }
        // A new paragraph after the last block (or in an empty note).
        if let Some(Spot::Gap(at)) = spot
            && !gap_placed
        {
            rows.push(gap(at, rows.is_empty(), window, cx));
        }
        // The link card closes once the caret leaves the link (unless it's
        // being typed in).
        if let Some(card) = &self.link_card {
            let typing = card.text_input.focus_handle(cx).is_focused(window) || card.url_input.focus_handle(cx).is_focused(window);
            let on_link = self.caret.is_some_and(|c| card.source.start <= c && c <= card.source.end);
            if !typing && !on_link && !card.hover {
                self.link_card = None;
            }
        }
        let card = self.link_card.as_ref().map(|card| {
            let theme = cx.theme();
            let url = card.url.clone();
            deferred(
                anchored().position(card.at).snap_to_window_with_margin(px(8.)).child(
                    v_flex()
                        .id("live-link-card")
                        .occlude()
                        .w(px(320.))
                        .p_1p5()
                        .gap_1()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .shadow_md()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.close_link_card(window, cx);
                            }
                        }))
                        .child(Input::new(&card.text_input).small())
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(div().flex_1().min_w_0().child(Input::new(&card.url_input).small()))
                                .when(!url.is_empty(), |row| {
                                    row.child(
                                        crate::ui::icon_button("live-link-open", IconName::ExternalLink, "Open Link")
                                            .xsmall()
                                            .compact()
                                            .on_click(cx.listener(move |this, _, window, cx| this.open_link(&url, window, cx))),
                                    )
                                    .child(
                                        crate::ui::icon_button("live-link-remove", IconName::CircleX, "Remove Link")
                                            .xsmall()
                                            .compact()
                                            .on_click(cx.listener(|this, _, window, cx| this.remove_link(window, cx))),
                                    )
                                }),
                        ),
                ),
            )
            .with_priority(1)
        });
        let content = v_flex()
            .id("live-page")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::soft_break))
            .on_action(cx.listener(|this, _: &Left, _, cx| {
                if !this.collapse(false, cx) { this.moving(false, cx, |t, cx| t.horizontal(false, cx)) }
            }))
            .on_action(cx.listener(|this, _: &Right, _, cx| {
                if !this.collapse(true, cx) { this.moving(false, cx, |t, cx| t.horizontal(true, cx)) }
            }))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.moving(false, cx, |t, cx| t.vertical(false, cx))))
            .on_action(cx.listener(|this, _: &Down, w, cx| {
                let before = this.caret;
                this.moving(false, cx, |t, cx| t.vertical(true, cx));
                if this.caret == before {
                    this.past_the_end(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Indent, w, cx| this.indent(false, w, cx)))
            .on_action(cx.listener(|this, _: &Outdent, w, cx| this.indent(true, w, cx)))
            .on_action(cx.listener(|this, _: &DeleteWordBack, w, cx| this.delete_back(false, w, cx)))
            .on_action(cx.listener(|this, _: &DeleteLineBack, w, cx| this.delete_back(true, w, cx)))
            .on_action(cx.listener(|this, _: &DeleteWordForward, w, cx| this.delete_forward(false, w, cx)))
            .on_action(cx.listener(|this, _: &DeleteLineForward, w, cx| this.delete_forward(true, w, cx)))
            .on_action(cx.listener(|this, _: &WordLeft, _, cx| this.moving(false, cx, |t, cx| t.word_move(false, cx))))
            .on_action(cx.listener(|this, _: &WordRight, _, cx| this.moving(false, cx, |t, cx| t.word_move(true, cx))))
            .on_action(cx.listener(|this, _: &LineStart, _, cx| this.moving(false, cx, |t, cx| t.line_move(false, cx))))
            .on_action(cx.listener(|this, _: &LineEnd, _, cx| this.moving(false, cx, |t, cx| t.line_move(true, cx))))
            .on_action(cx.listener(|this, _: &PageStart, _, cx| this.moving(false, cx, |t, cx| t.page_edge(false, cx))))
            .on_action(cx.listener(|this, _: &PageEnd, _, cx| this.moving(false, cx, |t, cx| t.page_edge(true, cx))))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, w, cx| this.mouse_drag(event, w, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.dragging = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.dragging = false))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| this.moving(true, cx, |t, cx| t.horizontal(false, cx))))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| this.moving(true, cx, |t, cx| t.horizontal(true, cx))))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.moving(true, cx, |t, cx| t.vertical(false, cx))))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.moving(true, cx, |t, cx| t.vertical(true, cx))))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| this.moving(true, cx, |t, cx| t.word_move(false, cx))))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| this.moving(true, cx, |t, cx| t.word_move(true, cx))))
            .on_action(cx.listener(|this, _: &SelectLineStart, _, cx| this.moving(true, cx, |t, cx| t.line_move(false, cx))))
            .on_action(cx.listener(|this, _: &SelectLineEnd, _, cx| this.moving(true, cx, |t, cx| t.line_move(true, cx))))
            .on_action(cx.listener(|this, _: &SelectPageStart, _, cx| this.moving(true, cx, |t, cx| t.page_edge(false, cx))))
            .on_action(cx.listener(|this, _: &SelectPageEnd, _, cx| this.moving(true, cx, |t, cx| t.page_edge(true, cx))))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| {
                if !this.copy(false, cx) { cx.propagate(); }
            }))
            .on_action(cx.listener(|this, _: &CopyMarkdown, _, cx| {
                if !this.copy(true, cx) { cx.propagate(); }
            }))
            .on_action(cx.listener(|this, _: &Cut, w, cx| {
                if this.copy(false, cx) { this.delete_selection(w, cx); } else { cx.propagate(); }
            }))
            .on_action(cx.listener(|this, _: &Paste, w, cx| this.paste(w, cx)))
            .on_action(cx.listener(|this, _: &Bold, w, cx| this.toggle_mark(Wrap::Strong, w, cx)))
            .on_action(cx.listener(|this, _: &Italic, w, cx| this.toggle_mark(Wrap::Emphasis, w, cx)))
            .on_action(cx.listener(|this, _: &Strike, w, cx| this.toggle_mark(Wrap::Delete, w, cx)))
            .on_action(cx.listener(|this, _: &InlineCode, w, cx| this.toggle_mark(Wrap::Code, w, cx)))
            .on_action(cx.listener(|this, _: &EditLink, w, cx| this.edit_link(w, cx)))
            .on_action(cx.listener(|this, _: &Heading1, w, cx| this.set_heading(1, w, cx)))
            .on_action(cx.listener(|this, _: &Heading2, w, cx| this.set_heading(2, w, cx)))
            .on_action(cx.listener(|this, _: &Heading3, w, cx| this.set_heading(3, w, cx)))
            .on_action(cx.listener(|this, _: &Heading4, w, cx| this.set_heading(4, w, cx)))
            .on_action(cx.listener(|this, _: &Heading5, w, cx| this.set_heading(5, w, cx)))
            .on_action(cx.listener(|this, _: &Heading6, w, cx| this.set_heading(6, w, cx)))
            .on_action(cx.listener(|this, _: &BodyText, w, cx| this.set_heading(0, w, cx)))
            .on_action(cx.listener(|this, _: &BulletList, w, cx| this.set_list("- ", w, cx)))
            .on_action(cx.listener(|this, _: &NumberedList, w, cx| this.set_list("1. ", w, cx)))
            .on_action(cx.listener(|this, _: &TaskList, w, cx| this.set_list("- [ ] ", w, cx)))
            .on_action(cx.listener(|this, _: &ToggleDone, w, cx| this.toggle_done(w, cx)))
            .on_action(cx.listener(|this, _: &QuoteLine, w, cx| this.quote_line(w, cx)))
            .on_action(cx.listener(|this, _: &CodeBlockLine, w, cx| this.code_block_line(w, cx)))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::stop_editing))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pl(px(crate::reader::TEXT_INSET_X))
            .pr_4()
            .pt(px(crate::reader::TEXT_INSET_Y))
            .pb_8()
            .font_family(reading.font.family(cx))
            .text_size(px(reading.size as f32))
            .children(rows)
            .children(card)
            // Below the last block: a click there writes a new paragraph at the end.
            .child(div().id("live-end").w_full().min_h(px(120.)).cursor_text().on_click(cx.listener(
                |this, _, window, cx| {
                    if !this.editor.read(cx).is_editable() {
                        return;
                    }
                    window.focus(&this.focus, cx);
                    let end = this.source.len();
                    this.caret = Some(match this.page.leaves.last() {
                        // Right after the last line of text, to keep typing it…
                        Some(last) if this.source[last.span.end..].trim().is_empty() && !this.source[last.span.start..].contains("\n\n") => {
                            last.flow.source_for(last.flow.text.len())
                        }
                        // …else a new paragraph at the end.
                        _ => end,
                    });
                    cx.notify();
                },
            )));
        // The selection, painted behind each line it covers.
        let mut decorations: HashMap<u64, Vec<(Range<usize>, Hsla)>> = HashMap::new();
        if let Some(selection) = self.selection().filter(|_| focused) {
            let color = cx.theme().selection;
            for (i, leaf) in self.page.leaves.iter().enumerate() {
                let (start, end) = (selection.start.max(leaf.span.start), selection.end.min(leaf.span.end));
                if start > end || (start == end && !(selection.start < leaf.span.start && leaf.span.end < selection.end)) {
                    continue;
                }
                let shown = leaf.flow.shown_for(start)..leaf.flow.shown_for(end);
                if !shown.is_empty() {
                    decorations.insert(leaf_key(i), vec![(shown, color)]);
                }
            }
        }
        LiveElement::new(cx.entity(), self.geometry.clone(), decorations, content.into_any_element())
    }
}

impl EntityInputHandler for LiveView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.shown_text()?;
        let text = text.as_str();
        let bytes = utf16_to_byte(text, range.start)..utf16_to_byte(text, range.end);
        *adjusted = Some(byte_to_utf16(text, bytes.start)..byte_to_utf16(text, bytes.end));
        Some(text[bytes].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let text = self.shown_text()?;
        let at = byte_to_utf16(&text, self.caret_in_active()?);
        Some(UTF16Selection { range: at..at, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if range.is_none() {
            self.delete_selection(window, cx);
        }
        let (Some(caret), Some(shown)) = (self.caret, self.shown_text()) else { return };
        // `/` on an empty line: the insert menu, as in the editor.
        if text == "/" && range.is_none() && shown.is_empty() && self.open_insert_menu(caret, window, cx) {
            return;
        }
        // In a table cell, a typed `|` is text, not a new column.
        let in_cell = matches!(self.spot(), Some(Spot::Leaf(i)) if matches!(self.page.leaves[i].kind, Kind::Cell { .. }));
        let escaped;
        let text = if in_cell && text.contains('|') {
            escaped = text.replace('|', "\\|");
            escaped.as_str()
        } else {
            text
        };
        let target = match range {
            Some(r) => {
                let (Some(start), Some(end)) = (
                    self.source_at(utf16_to_byte(&shown, r.start)),
                    self.source_at(utf16_to_byte(&shown, r.end)),
                ) else {
                    return;
                };
                start..end.max(start)
            }
            None => caret..caret,
        };
        self.edit(target, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range, text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let local = self.caret_in_active()?;
        Some(caret_bounds(&self.active_geometry()?, local))
    }

    fn character_index_for_point(&mut self, at: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let text = self.shown_text()?;
        let index = self.active_geometry()?.index_for_position(at);
        Some(byte_to_utf16(&text, index))
    }
}

/// The caret at `index`: as tall as the font, centred in the line box as
/// GPUI centres the letters.
pub(crate) fn caret_bounds(geometry: &LeafGeometry, index: usize) -> Bounds<Pixels> {
    let (line, height) = (geometry.line_height(), geometry.caret_height());
    let mut top = geometry.position_for_index(index);
    top.y += (line - height) / 2.;
    Bounds::new(top, size(px(1.5), height))
}

/// `%20` and friends back to their characters, for a wiki link's note name.
fn urlencoding_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn on_blank_line(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    text[start..end].trim().is_empty() || quote_line(text, at).is_some()
}

/// The line at `at` when it's an empty line of a quote (`>` alone): a new
/// paragraph is being started in the quote.
fn quote_line(text: &str, at: usize) -> Option<Range<usize>> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    let line = text[start..end].trim();
    (line.starts_with('>') && line.trim_start_matches('>').trim().is_empty()).then_some(start..end)
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn previous_boundary(text: &str, at: usize) -> usize {
    text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_boundary(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
}

fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (i, c) in text.char_indices() {
        if count >= utf16 {
            return i;
        }
        count += c.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..floor_boundary(text, byte)].chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    #[test]
    fn utf16_and_bytes_convert_both_ways() {
        let text = "aé😀b";
        assert_eq!(super::utf16_to_byte(text, 2), 3);
        assert_eq!(super::utf16_to_byte(text, 4), 7);
        assert_eq!(super::byte_to_utf16(text, 7), 4);
        assert_eq!(super::previous_boundary(text, 7), 3);
        assert_eq!(super::next_boundary(text, 3), 7);
    }
}
