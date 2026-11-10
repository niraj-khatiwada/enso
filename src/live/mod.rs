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

mod actions;
mod element;
mod flow;
mod page;

use crate::{Enso, rich, typography};
use crate::ui::SurfaceExt;
use element::{GAP_LEAF, LiveElement, leaf_key};
use flow::{Flow, InlineImage, LinkAt, MarkAt, Wrap};
use gpui_kit::base::text::editor_host::{
    BlockNode, CodeBlock, FlowPosition, InlineNode, LeafGeometry, Paragraph, RenderContext,
};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::{EditorState, Input, InputEvent, InputState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
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
        StopEditing, ToggleMarkdown, OpenActions, Undo, Redo, DeleteWordBack, DeleteWordForward, DeleteLineBack,
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
        // The editor's formatting keys. ⌘E is the app's (View and Edit);
        // inline code is in ⌘/. ⌘⇧C copies Markdown.
        KeyBinding::new(primary("cmd-b", "ctrl-b"), Bold, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-i", "ctrl-i"), Italic, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-x", "ctrl-shift-x"), Strike, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-shift-e", "ctrl-shift-e"), ToggleMarkdown, Some(CONTEXT)),
        KeyBinding::new(primary("cmd-/", "ctrl-/"), OpenActions, Some(CONTEXT)),
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

/// The highlight card: the highlight, where it shows, and whether hovering
/// opened it (then it goes when the pointer leaves).
struct MarkCard {
    mark: MarkAt,
    at: Point<Pixels>,
    hover: bool,
    /// Held open (its colour picker in use): it stays until a colour is
    /// chosen, Escape, or a click elsewhere.
    pinned: bool,
    picker: Entity<ColorPickerState>,
    _picker: Subscription,
}

/// The text colour card (⌘/ › Text Color › Custom Color…): where it works
/// (the selection, kept while its picker has the keys), and where it shows.
struct TextColorCard {
    scope: actions::Scope,
    at: Point<Pixels>,
    current: Option<Hsla>,
    picker: Entity<ColorPickerState>,
    _picker: Subscription,
}

/// An image in the text being resized by a handle: where the drag began,
/// the width then and now, and which side's handle.
struct ImageDrag {
    image: InlineImage,
    from: Pixels,
    start: Pixels,
    width: Pixels,
    left: bool,
}

/// The ⌘/ menu: what it offers, where it was opened, and where it shows.
struct ActionMenu {
    state: Entity<CommandState>,
    buttons: Vec<actions::Button>,
    actions: Vec<actions::Action>,
    scope: actions::Scope,
    at: Point<Pixels>,
    /// The submenu open beside it (Convert To, Color, Case) and its search.
    sub: Option<(&'static str, Entity<CommandState>)>,
    /// Searching switches the list between submenus and every action.
    _query: Subscription,
}

/// The math card (⌘K on inline math): its Markdown, its TeX being edited,
/// and where it shows.
struct MathCard {
    source: Range<usize>,
    input: Entity<InputState>,
    at: Point<Pixels>,
    _subscription: Subscription,
}

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

/// A change to a table's shape.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TableOp {
    RowAbove,
    RowBelow,
    DeleteRow,
    ColumnLeft,
    ColumnRight,
    DeleteColumn,
    Align(Align),
    /// The + under the table.
    AddRow,
    /// The + beside it.
    AddColumn,
}

/// A table as its cells' Markdown, to change its shape and write it back.
struct Grid {
    range: Range<usize>,
    cells: Vec<Vec<String>>,
    aligns: Vec<Align>,
}

impl Grid {
    fn markdown(&self) -> String {
        let width = self.cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let row = |cells: &[String]| {
            let mut line = String::from("|");
            for i in 0..width {
                let cell = cells.get(i).map_or("", String::as_str);
                line.push_str(&format!(" {cell} |"));
            }
            line
        };
        let mut lines = vec![row(self.cells.first().map_or(&[][..], Vec::as_slice))];
        let mut delimiter = String::from("|");
        for i in 0..width {
            delimiter.push_str(match self.aligns.get(i).copied().unwrap_or(Align::None) {
                Align::Left => " :--- |",
                Align::Center => " :---: |",
                Align::Right => " ---: |",
                Align::None => " --- |",
            });
        }
        lines.push(delimiter);
        for cells in self.cells.iter().skip(1) {
            lines.push(row(cells));
        }
        lines.join("\n")
    }
}

/// Keys of the empty lines' layout (one per line, by where it starts):
/// apart from every line of text's.
const BLANK_LEAF: u64 = 1 << 40;

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
    math_card: Option<MathCard>,
    mark_card: Option<MarkCard>,
    /// The image in the text under the pointer (its handles show), and its
    /// line.
    hovered_image: Option<(usize, InlineImage)>,
    image_drag: Option<ImageDrag>,
    text_color_card: Option<TextColorCard>,
    /// A press landed in the ⌘/ submenu (not outside the menu).
    sub_pressed: bool,
    action_menu: Option<ActionMenu>,
    /// The table card (⌘K in a cell): the cell, and where the card shows.
    table_card: Option<(usize, Point<Pixels>)>,
    /// The table cell under the pointer: its toolbar shows.
    hovered_cell: Option<usize>,
    /// A link the pointer rests on, and the wait before its card shows.
    hovering: Option<(Range<usize>, Task<()>)>,
    /// The block shown as its Markdown (⌘⇧E), edited as text: its range,
    /// kept up to date as it's typed in.
    raw: Option<Range<usize>>,
    /// A block without text (an image, a diagram) picked by a click, by its
    /// start: Backspace or Delete removes it.
    picked: Option<usize>,
    /// An edit of its own in flight (its undo still applies after it).
    own_edit: bool,
    /// Find's matches on the page (in the note), and which is the current.
    found: Vec<Range<usize>>,
    found_current: Option<usize>,
    /// Where the picked block was pressed, for menus about it.
    picked_at: Option<Point<Pixels>>,
    /// The pointer is over the link card: it stays.
    over_card: bool,
    /// The hover card closing, a moment after the pointer left the link (so
    /// the pointer can cross over to it).
    closing: Option<Task<()>>,
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
            table_card: None,
            math_card: None,
            mark_card: None,
            hovered_image: None,
            image_drag: None,
            text_color_card: None,
            sub_pressed: false,
            action_menu: None,
            hovered_cell: None,
            hovering: None,
            over_card: false,
            raw: None,
            picked: None,
            picked_at: None,
            found: Vec::new(),
            own_edit: false,
            found_current: None,
            closing: None,
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
            // Changed from elsewhere (the editor beside it in Split, the file
            // on disk): this page's undo no longer applies to it.
            if !std::mem::take(&mut self.own_edit) {
                self.undo.clear();
                self.redo.clear();
            }
            self.source = value.to_string();
            self.revision += 1;
            // Changed from elsewhere: the raw block may be gone.
            if self.raw.as_ref().is_some_and(|r| {
                r.end > self.source.len() || !self.source.is_char_boundary(r.start) || !self.source.is_char_boundary(r.end)
            }) {
                self.raw = None;
            }
            self.page = Page::read_with_raw(&self.source, self.raw.clone());
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

    /// Where the text of the line holding `at` was drawn.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn text_bounds_for_test(&self, at: usize) -> Bounds<Pixels> {
        let leaf = self.page.leaves.iter().position(|l| l.span.start <= at && at <= l.span.end).unwrap();
        self.geometry.borrow()[&leaf_key(leaf)].bounds()
    }

    /// A point inside the link card.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn link_card_point_for_test(&self) -> Option<Point<Pixels>> {
        self.link_card.as_ref().map(|c| point(c.at.x + px(40.), c.at.y + px(20.)))
    }

    /// A click on the block starting at `start` (an image).
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn click_block_for_test(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.click_other(start, None, window, cx);
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn picked_for_test(&self) -> Option<usize> {
        self.picked
    }

    /// The math card's input, while it shows.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn math_input_for_test(&self) -> Option<Entity<InputState>> {
        self.math_card.as_ref().map(|c| c.input.clone())
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn raw_for_test(&self) -> Option<Range<usize>> {
        self.raw.clone()
    }

    /// Whether the highlight card shows, and whether hovering opened it.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn mark_card_for_test(&self) -> Option<bool> {
        self.mark_card.as_ref().map(|c| c.hover)
    }

    /// A swatch (by its place) or Remove (`None`) pressed on the highlight card.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn recolor_for_test(&mut self, swatch: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let color = swatch.map(|i| rgb(rich::HIGHLIGHTS[i].1).into());
        self.recolor_mark(color, window, cx);
    }

    /// A point on the first highlight's text.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn mark_point_for_test(&self) -> Option<Point<Pixels>> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            let at = (0..leaf.flow.text.len()).find(|a| leaf.flow.mark_at(*a, true).is_some())?;
            let g = geometry.get(&leaf_key(i))?;
            let p = g.position_for_index(at + 1);
            Some(point(p.x, p.y + g.line_height() / 2.))
        })
    }

    /// The ⌘/ menu's actions, while it shows.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn actions_for_test(&self) -> Option<Vec<String>> {
        self.action_menu.as_ref().map(|m| m.actions.iter().map(|a| a.label.clone()).collect())
    }

    /// The ⌘/ menu's formatting buttons: each one's label and whether it's lit.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn buttons_for_test(&self) -> Vec<(String, bool)> {
        self.action_menu.as_ref().map_or(Vec::new(), |m| m.buttons.iter().map(|b| (b.label.to_string(), b.active)).collect())
    }

    /// Press the ⌘/ menu's button labelled `label`.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn press_for_test(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) {
        let menu = self.action_menu.as_ref().expect("the actions menu");
        let act = menu.buttons.iter().find(|b| b.label == label).unwrap_or_else(|| panic!("no {label}")).act.clone();
        self.press_button(act, window, cx);
    }

    /// The ⌘/ menu's open submenu.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn sub_for_test(&self) -> Option<&'static str> {
        self.action_menu.as_ref().and_then(|m| m.sub.as_ref().map(|(name, _)| *name))
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn open_sub_for_test(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_sub(name, window, cx);
    }

    /// Choose the ⌘/ menu's action labelled `label`.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn choose_for_test(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) {
        let menu = self.action_menu.as_ref().expect("the actions menu");
        let index = menu.actions.iter().position(|a| a.label == label).unwrap_or_else(|| panic!("no {label}"));
        self.choose_action(index, window, cx);
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn select_for_test(&mut self, range: Range<usize>) {
        self.anchor = Some(range.start);
        self.caret = Some(range.end);
    }

    /// The Edit button of the block starting at `start` pressed.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn edit_block_for_test(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_raw(start, window, cx);
    }

    /// Find's matches on the page, and the current one.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn found_for_test(&self) -> (Vec<Range<usize>>, Option<usize>) {
        (self.found.clone(), self.found_current)
    }

    /// Where the hovered image in the text is drawn, while its handles show.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn hovered_image_for_test(&self) -> Option<Bounds<Pixels>> {
        let (leaf, image) = self.hovered_image.as_ref()?;
        self.image_rect(*leaf, image)
    }

    /// A point on the first image in the text.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn image_point_for_test(&self) -> Option<Point<Pixels>> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            let image = leaf.flow.images.first()?;
            let g = geometry.get(&leaf_key(i))?;
            let start = g.position_for_index(image.shown.start);
            let end = g.position_for_index(image.shown.end);
            Some(point((start.x + end.x) / 2., start.y + g.line_height() / 2.))
        })
    }

    /// A colour picked in the text colour card (which must be open).
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn pick_text_color_for_test(&mut self, color: Hsla, window: &mut Window, cx: &mut Context<Self>) {
        assert!(self.text_color_card.is_some(), "the text colour card");
        self.choose_text_color(Some(color), window, cx);
    }

    /// How many steps this page can undo.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn undo_steps_for_test(&self) -> usize {
        self.undo.len()
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn scroll_for_test(&self) -> Point<Pixels> {
        self.scroll.offset()
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn set_scroll_for_test(&self, y: Pixels) {
        self.scroll.set_offset(point(px(0.), -y));
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn table_card_for_test(&self) -> bool {
        self.table_card.is_some()
    }

    /// A table change at the caret's cell, as its card or + bar would make it.
    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn table_op_for_test(&mut self, op: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Spot::Leaf(i)) = self.spot() else { panic!("not in a cell") };
        let Kind::Cell { row, col } = self.page.leaves[i].kind else { panic!("not in a cell") };
        let start = self.page.blocks.iter().find_map(|(range, shape)| match shape {
            Shape::Table { rows, .. } if rows.iter().flatten().any(|l| *l == i) => Some(range.start),
            _ => None,
        });
        let op = match op {
            "row-above" => TableOp::RowAbove,
            "delete-row" => TableOp::DeleteRow,
            "column-right" => TableOp::ColumnRight,
            "delete-column" => TableOp::DeleteColumn,
            "align-center" => TableOp::Align(Align::Center),
            "add-row" => TableOp::AddRow,
            "add-column" => TableOp::AddColumn,
            _ => panic!("{op}"),
        };
        self.table_op(start.unwrap(), row, col, op, window, cx);
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn set_caret_for_test(&mut self, caret: usize) {
        self.caret = Some(caret);
        self.anchor = None;
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
        self.picked = None;
        self.math_card = None;
        self.mark_card = None;
        self.text_color_card = None;
        self.action_menu = None;
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
        self.follow_in_editor(at, window, cx);
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

    /// The image in the text under the pointer, and its line.
    fn image_under(&self, at: Point<Pixels>) -> Option<(usize, InlineImage)> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            if leaf.flow.images.is_empty() {
                return None;
            }
            let g = geometry.get(&leaf_key(i))?;
            let atom = g.atom_at(at)?;
            leaf.flow.images.iter().find(|image| image.shown.start == atom.start).map(|image| (i, image.clone()))
        })
    }

    /// Where an image in the text is drawn (from its line's last layout):
    /// its left from its place in the text, its other edges found by asking
    /// what's drawn where.
    fn image_rect(&self, leaf: usize, image: &InlineImage) -> Option<Bounds<Pixels>> {
        let geometry = self.geometry.borrow();
        let g = geometry.get(&leaf_key(leaf))?;
        let start = g.position_for_index(image.shown.start);
        let on = |x: Pixels, y: Pixels| g.atom_at(point(x, y)).is_some_and(|a| a.start == image.shown.start);
        let x = start.x + px(1.);
        let bounds = g.bounds();
        // A point on it: down from where it starts.
        let mut y = start.y;
        while y < bounds.bottom() && !on(x, y) {
            y += px(2.);
        }
        if !on(x, y) {
            return None;
        }
        // Each edge: halving the gap between a point on it and one off it.
        let edge = |inside: Pixels, outside: Pixels, on_it: &dyn Fn(Pixels) -> bool| {
            let (mut a, mut b) = (inside, outside);
            for _ in 0..24 {
                let mid = (a + b) / 2.;
                if on_it(mid) { a = mid } else { b = mid }
            }
            a
        };
        let right = edge(x, bounds.right() + px(1.), &|r| on(r, y));
        let top = edge(y, bounds.top() - px(1.), &|t| on(x, t));
        let bottom = edge(y, bounds.bottom() + px(1.), &|b| on(x, b));
        Some(Bounds::from_corners(point(start.x, top), point(right, bottom)))
    }

    /// A press on an image's handle: resizing it begins.
    fn start_image_drag(&mut self, image: InlineImage, width: Pixels, left: bool, at: Pixels, cx: &mut Context<Self>) {
        self.image_drag = Some(ImageDrag { image, from: at, start: width, width, left });
        cx.notify();
    }

    /// The highlight under the pointer, and its line.
    fn mark_under(&self, at: Point<Pixels>) -> Option<(usize, MarkAt)> {
        let geometry = self.geometry.borrow();
        self.page.leaves.iter().enumerate().find_map(|(i, leaf)| {
            let g = geometry.get(&leaf_key(i))?;
            if !g.bounds().contains(&at) {
                return None;
            }
            leaf.flow.mark_at(g.index_for_position(at), true).map(|mark| (i, mark))
        })
    }

    /// The highlight's card, under its start: its colours, and Remove.
    fn open_mark_card(&mut self, mark: MarkAt, leaf: usize, hover: bool, pinned: bool, window: &mut Window, cx: &mut Context<Self>) {
        let at = {
            let geometry = self.geometry.borrow();
            let flow = &self.page.leaves[leaf].flow;
            geometry.get(&leaf_key(leaf)).map(|g| {
                let top = g.position_for_index(flow.shown_for(mark.open.end));
                point(top.x, top.y + g.line_height() + px(4.))
            })
        }
        .unwrap_or(point(px(120.), px(120.)));
        self.over_card = false;
        self.closing = None;
        self.link_card = None;
        let color = mark.color;
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(color));
        let subscription = cx.subscribe_in(&picker, window, |this, _, event: &ColorPickerEvent, window, cx| {
            let ColorPickerEvent::Change(Some(color)) = event else { return };
            this.recolor_mark(Some(*color), window, cx);
        });
        self.mark_card = Some(MarkCard { mark, at, hover, pinned, picker, _picker: subscription });
        cx.notify();
    }

    /// The text colour card under the caret: the text colours and a picker.
    pub(super) fn open_text_color_card(&mut self, current: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        let at = self
            .active_key()
            .zip(self.caret_in_active())
            .and_then(|(key, local)| {
                let geometry = self.geometry.borrow();
                let g = geometry.get(&key)?;
                let p = g.position_for_index(local);
                Some(point(p.x, p.y + g.line_height() + px(6.)))
            })
            .unwrap_or(point(px(120.), px(120.)));
        let ink = current.unwrap_or(cx.theme().foreground);
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(ink));
        let subscription = cx.subscribe_in(&picker, window, |this, _, event: &ColorPickerEvent, window, cx| {
            let ColorPickerEvent::Change(Some(color)) = event else { return };
            this.choose_text_color(Some(*color), window, cx);
        });
        let scope = actions::Scope { caret: self.caret, anchor: self.anchor, picked: None };
        self.text_color_card = Some(TextColorCard { scope, at, current, picker, _picker: subscription });
        cx.notify();
    }

    /// A colour from the text colour card (`None`: the page's own), on the
    /// text it was opened for.
    fn choose_text_color(&mut self, color: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.text_color_card.take() else { return };
        self.focus_page(window, cx);
        self.caret = card.scope.caret;
        self.anchor = card.scope.anchor;
        self.parked = None;
        self.apply_text_color(color, window, cx);
    }

    /// The highlight card for the highlight at the caret. False when the
    /// caret isn't in one.
    fn open_mark_card_at_caret(&mut self, pinned: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (Some(caret), Some(Spot::Leaf(i))) = (self.caret, self.spot()) else { return false };
        let flow = &self.page.leaves[i].flow;
        let Some(mark) = flow.mark_at(flow.shown_for(caret), false) else { return false };
        self.open_mark_card(mark, i, false, pinned, window, cx);
        true
    }

    /// ⌘/: what can be done to what the caret is in.
    fn open_actions(&mut self, _: &OpenActions, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.read(cx).is_editable() {
            return;
        }
        if self.action_menu.take().is_some() {
            self.focus_page(window, cx);
            cx.notify();
            return;
        }
        let actions = self.actions(self.in_split(cx));
        let buttons = self.buttons();
        if actions.is_empty() && buttons.is_empty() {
            return;
        }
        // Under the caret (or the picked block's top).
        let at = self
            .active_key()
            .zip(self.caret_in_active())
            .and_then(|(key, local)| {
                let geometry = self.geometry.borrow();
                let g = geometry.get(&key)?;
                let p = g.position_for_index(local);
                Some(point(p.x, p.y + g.line_height() + px(6.)))
            })
            .or_else(|| self.picked.and(self.picked_at).map(|p| point(p.x, p.y + px(8.))))
            .or_else(|| {
                // A picked block reached by the keyboard: under the line
                // before it.
                let start = self.picked?;
                let i = self.page.leaves.iter().rposition(|l| l.span.end <= start)?;
                let geometry = self.geometry.borrow();
                let g = geometry.get(&leaf_key(i))?;
                Some(point(g.bounds().left(), g.bounds().bottom() + px(6.)))
            })
            .unwrap_or(point(px(120.), px(120.)));
        let scope = actions::Scope { caret: self.caret, anchor: self.anchor, picked: self.picked };
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |state, cx| state.focus(window, cx));
        self.link_card = None;
        self.mark_card = None;
        self.table_card = None;
        let query = cx.observe(&state, |_, _, cx| cx.notify());
        self.action_menu = Some(ActionMenu { state, buttons, actions, scope, at, sub: None, _query: query });
        cx.notify();
    }

    fn close_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.action_menu.take() {
            self.caret = menu.scope.caret;
            self.anchor = menu.scope.anchor;
            self.picked = menu.scope.picked;
            self.parked = None;
            self.focus_page(window, cx);
            cx.notify();
        }
    }

    /// Open submenu `name` beside the menu, its search focused.
    fn open_sub(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_sub(name, true, window, cx);
    }

    /// Show submenu `name` beside the menu; `focus`: its search takes the
    /// keys (chosen with the keyboard), else the menu keeps them (hovered).
    fn show_sub(&mut self, name: &'static str, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.as_mut() else { return };
        let state = match &menu.sub {
            Some((open, state)) if *open == name => state.clone(),
            _ => {
                let state = cx.new(|cx| CommandState::new(window, cx));
                menu.sub = Some((name, state.clone()));
                cx.notify();
                state
            }
        };
        if focus {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    /// Back from the submenu to the menu.
    fn close_sub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.as_mut() else { return };
        if menu.sub.take().is_some() {
            menu.state.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
        }
    }


    /// Run one of the menu's formatting buttons.
    fn press_button(&mut self, act: actions::Act, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.take() else { return };
        self.run_action(act, menu.scope, window, cx);
    }

    /// Run the menu's `index`th action.
    fn choose_action(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.take() else { return };
        let Some(action) = menu.actions.get(index).cloned() else { return };
        self.run_action(action.act, menu.scope, window, cx);
    }

    /// Recolour the card's highlight (`None`: remove it, keeping its text).
    fn recolor_mark(&mut self, color: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.mark_card.take() else { return };
        let mark = card.mark;
        if self.source.get(mark.source.clone()).is_none() {
            return;
        }
        let caret = self.caret;
        self.focus_page(window, cx);
        match color {
            Some(color) => {
                let tag = format!("<mark color=\"{}\">", rich::highlight_hex(color));
                let delta = tag.len() as isize - mark.open.len() as isize;
                self.edit(mark.open.clone(), &tag, window, cx);
                self.caret = caret.map(|c| if c >= mark.open.end { (c as isize + delta) as usize } else { c });
            }
            None => {
                self.edit(mark.close.clone(), "", window, cx);
                self.last_edit = Some(Instant::now());
                self.edit(mark.open.clone(), "", window, cx);
                self.caret = caret.map(|c| {
                    let c = if c >= mark.close.end { c - mark.close.len() } else { c.min(mark.close.start) };
                    if c >= mark.open.end { c - mark.open.len() } else { c.min(mark.open.start) }
                });
            }
        }
        cx.notify();
    }

    /// The pointer resting on a link for a moment (0.8 s) shows its card; leaving
    /// the link (and not onto the card) puts a hover card away.
    fn hover(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        // An image in the text: its resize handles.
        let image = self.image_under(at).filter(|_| self.editor.read(cx).is_editable());
        if self.hovered_image.as_ref().map(|(_, i)| &i.source) != image.as_ref().map(|(_, i)| &i.source) {
            self.hovered_image = image.clone();
            cx.notify();
        }
        if image.is_some() {
            self.hovering = None;
            return;
        }
        let target = match self.link_under(at) {
            Some((leaf, link)) => Some((leaf, Ok(link))),
            None => self.mark_under(at).map(|(leaf, mark)| (leaf, Err(mark))),
        };
        match target {
            Some((leaf, target)) => {
                let source = match &target {
                    Ok(link) => link.source.clone(),
                    Err(mark) => mark.source.clone(),
                };
                let shown = self.link_card.as_ref().is_some_and(|c| c.source == source)
                    || self.mark_card.as_ref().is_some_and(|c| c.mark.source == source);
                if shown {
                    self.closing = None;
                }
                let waiting = self.hovering.as_ref().is_some_and(|(r, _)| *r == source);
                if shown || waiting {
                    return;
                }
                let task = cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(800)).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.hovering = None;
                        match target {
                            Ok(link) => this.open_link_card(link, leaf, false, window, cx),
                            Err(mark) => this.open_mark_card(mark, leaf, true, false, window, cx),
                        }
                    });
                });
                self.hovering = Some((source, task));
            }
            None => {
                self.hovering = None;
                self.close_hover_card_soon(window, cx);
            }
        }
    }

    /// Close the hover card shortly, unless the pointer reaches it (or comes
    /// back to the link) first, or it's being typed in.
    fn close_hover_card_soon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hovered = self.link_card.as_ref().is_some_and(|c| c.hover) || self.mark_card.as_ref().is_some_and(|c| c.hover);
        if self.over_card || self.closing.is_some() || !hovered {
            return;
        }
        self.closing = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.closing = None;
                if this.mark_card.as_ref().is_some_and(|c| c.hover && !c.pinned) && !this.over_card {
                    this.mark_card = None;
                    cx.notify();
                }
                let Some(card) = &this.link_card else { return };
                let typing = card.text_input.focus_handle(cx).is_focused(window) || card.url_input.focus_handle(cx).is_focused(window);
                if card.hover && !typing && !this.over_card {
                    this.link_card = None;
                    cx.notify();
                }
            });
        }));
    }

    fn mouse_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        // An image's handle dragged: it's drawn at the new width as it goes.
        let line = self.line_width();
        if let Some(drag) = self.image_drag.as_mut() {
            let moved = event.position.x - drag.from;
            drag.width = (drag.start + if drag.left { -moved } else { moved }).max(px(24.)).min(line);
            let (source, width) = (drag.image.source.clone(), drag.width);
            for leaf in &mut self.page.leaves {
                for image in &mut leaf.flow.images {
                    if image.source == source {
                        image.width = Some(format!("{}", f32::from(width).round()));
                    }
                }
            }
            cx.notify();
            return;
        }
        if !self.dragging || event.pressed_button != Some(MouseButton::Left) {
            self.hover(event.position, window, cx);
            return;
        }
        let mut target = self.leaf_near(event.position).and_then(|leaf| self.source_at_point(leaf, event.position));
        // Started in a table cell: the selection stays in that cell.
        if let Some(cell) = self.anchor.and_then(|a| self.page.leaf_at(a)).filter(|i| matches!(self.page.leaves[*i].kind, Kind::Cell { .. })) {
            let span = self.page.leaves[cell].span.clone();
            target = target.map(|at| at.clamp(span.start, span.end));
        }
        if let Some(at) = target
            && self.caret != Some(at)
        {
            self.caret = Some(at);
            cx.notify();
        }
    }

    /// A click on a block typed into only through its own controls (a table,
    /// an image): editing stops.
    fn click_other(&mut self, start: usize, at: Option<Point<Pixels>>, window: &mut Window, cx: &mut Context<Self>) {
        self.picked_at = at;
        if self.editor.read(cx).value().as_ref() != self.source.as_str() {
            self.refresh(cx);
        }
        self.anchor = None;
        self.caret = None;
        self.parked = None;
        self.picked = self.editor.read(cx).is_editable().then_some(start);
        window.focus(&self.focus, cx);
        self.follow_in_editor(start, window, cx);
        cx.notify();
    }

    /// Backspace (or Delete) on a paragraph's only letter, the paragraph
    /// on a line of its own after a blank line: it becomes an empty line
    /// again, as it was before it was typed, not a blank line more.
    fn empty_line_again(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (Some(caret), Some(Spot::Leaf(i))) = (self.caret, self.spot()) else { return false };
        let leaf = &self.page.leaves[i];
        let Kind::Text { block } = &leaf.kind else { return false };
        let flow = &leaf.flow;
        let at = flow.shown_for(caret);
        let one = flow.text.chars().count() == 1;
        let on_it = if forward { at == 0 } else { at == flow.text.len() };
        if !one || !on_it || flow.heading.is_some() || block.start < 2 || !self.source[..block.start].ends_with("\n\n") {
            return false;
        }
        let end = block.end.max(leaf.span.end);
        let gone = block.start - 1..end;
        self.edit(gone.clone(), "", window, cx);
        self.caret = Some(gone.start);
        cx.notify();
        true
    }

    /// Whether the selection is drawn: while the page has the keys, and
    /// while the ⌘/ menu (working on it) has them.
    fn shows_selection(&self, focused: bool) -> bool {
        focused || self.action_menu.is_some()
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub(crate) fn selection_drawn_for_test(&self, window: &Window) -> bool {
        self.selection().is_some() && self.shows_selection(self.focus.is_focused(window))
    }

    /// The block starting at `start` as its Markdown, the caret at its
    /// content (past a fence's first line).
    fn edit_raw(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.read(cx).is_editable() {
            return;
        }
        // Split: its Markdown is beside the page; the editor goes there.
        if self.in_split(cx) {
            let content = self.source[start..].find('\n').map_or(start, |n| start + n + 1).min(self.source.len());
            let block = self.block_markdown(start).unwrap_or(start..start);
            let (owner, editor) = (self.owner.clone(), self.editor_id);
            window.defer(cx, move |window, cx| {
                let _ = owner.update(cx, |app, cx| app.live_clicked(editor, content, Some(block.clone()), true, window, cx));
            });
            return;
        }
        let Some((range, _)) = self.page.blocks.iter().find(|(r, _)| r.start == start).cloned() else { return };
        self.raw = Some(range.clone());
        self.page = Page::read_with_raw(&self.source, self.raw.clone());
        self.picked = None;
        self.anchor = None;
        // What's typed in it undoes apart from what came before.
        self.last_edit = None;
        let text = &self.source[range.clone()];
        let fenced = text.starts_with("```") || text.starts_with("~~~") || text.starts_with("$$");
        let content = if fenced { text.find('\n').map_or(range.start, |n| range.start + n + 1) } else { range.start };
        self.caret = Some(content.min(range.end));
        self.parked = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Find: where `query` is in the page's text, in the note.
    pub(crate) fn find_all(&self, query: &str, limit: usize) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        for leaf in &self.page.leaves {
            for shown in crate::search::find_all(&leaf.flow.text, query, limit.saturating_sub(found.len())) {
                if let Some(span) = leaf.flow.char_span(shown) {
                    found.push(span);
                }
            }
            if found.len() >= limit {
                break;
            }
        }
        found
    }

    /// Find's matches painted (`current` stronger) and the current one
    /// brought into view.
    pub(crate) fn show_found(&mut self, found: Vec<Range<usize>>, current: Option<usize>, cx: &mut Context<Self>) {
        let at = current.and_then(|i| found.get(i)).map(|r| r.start);
        self.found = found;
        self.found_current = current;
        if let Some(at) = at {
            self.reveal(at);
        }
        cx.notify();
    }

    /// Split: the editor's caret at `at`; the same place brought into view.
    pub(crate) fn reveal_source(&mut self, at: usize, cx: &mut Context<Self>) {
        self.refresh(cx);
        let at = self.page.leaf_at(at).map_or_else(
            || self.page.leaves.iter().find(|l| l.span.start >= at).map_or(at, |l| l.span.start),
            |_| at,
        );
        self.reveal(at);
        cx.notify();
    }

    pub(crate) fn clear_found(&mut self, cx: &mut Context<Self>) {
        if !self.found.is_empty() {
            self.found.clear();
            self.found_current = None;
            cx.notify();
        }
    }

    /// The outline's heading at `at` (in the note): the caret on it, in view.
    pub(crate) fn jump_to(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let leaf = self.page.leaf_at(at).or_else(|| self.page.leaves.iter().position(|l| l.span.start >= at));
        let Some(leaf) = leaf else { return };
        let flow = &self.page.leaves[leaf].flow;
        let to = flow.source_for(0);
        self.anchor = None;
        self.picked = None;
        self.caret = Some(to);
        self.parked = None;
        self.reveal(to);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Scroll so the line holding `at` (in the note) is in view, a third of
    /// the way down, when it isn't already.
    fn reveal(&self, at: usize) {
        let Some(leaf) = self.page.leaf_at(at) else { return };
        let geometry = self.geometry.borrow();
        let Some(g) = geometry.get(&leaf_key(leaf)) else { return };
        let flow = &self.page.leaves[leaf].flow;
        let y = g.position_for_index(flow.shown_for(at)).y;
        let view = self.scroll.bounds();
        let margin = g.line_height() * 2.;
        if y > view.top() + margin && y + margin < view.bottom() {
            return;
        }
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset();
        let moved = offset.y - (y - (view.top() + view.size.height / 3.));
        self.scroll.set_offset(point(offset.x, moved.min(px(0.)).max(-max.y.abs())));
    }

    /// How wide a line of text is on the page.
    fn line_width(&self) -> Pixels {
        let page = self.scroll.bounds().size.width - px(crate::reader::TEXT_INSET_X) - px(16.);
        if page > px(80.) { page } else { px(680.) }
    }

    /// The button let go: a drag's selection ends; an image's handle sets
    /// its new width.
    fn mouse_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dragging = false;
        if let Some(drag) = self.image_drag.take() {
            let width = f32::from(drag.width).round() as u16;
            self.resize_image(&drag.image, Some(width.max(1)), window, cx);
        }
    }

    /// A press on an empty line: the caret there.
    fn click_blank(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.read(cx).value().as_ref() != self.source.as_str() {
            self.refresh(cx);
            cx.notify();
            return;
        }
        self.parked = None;
        self.picked = None;
        self.anchor = None;
        self.caret = Some(at.min(self.source.len()));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Whether the page is Split's, beside the editor (where the Markdown
    /// is already shown).
    pub(super) fn in_split(&self, cx: &App) -> bool {
        self.owner.upgrade().is_some_and(|app| app.read(cx).mode == crate::Mode::Split)
    }

    /// The Markdown of what's at `at` (in the note): a list item's line, a
    /// table's cell, else the whole block.
    fn block_markdown(&self, at: usize) -> Option<Range<usize>> {
        if let Some(leaf) = self.page.leaf_at(at) {
            match &self.page.leaves[leaf].kind {
                Kind::Item(item) => {
                    let end = self.source[item.line_start..].find('\n').map_or(self.source.len(), |n| item.line_start + n);
                    return Some(item.line_start..end);
                }
                Kind::Cell { .. } => return Some(self.page.leaves[leaf].span.clone()),
                _ => {}
            }
        }
        self.page.blocks.iter().find(|(r, _)| r.start <= at && at <= r.end).map(|(r, _)| r.clone())
    }

    /// Split: the editor beside goes to `at` too (a click on the page).
    fn follow_in_editor(&self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.in_split(cx) {
            return;
        }
        let editor = self.editor_id;
        let owner = self.owner.clone();
        let block = self.block_markdown(at);
        // After this event: the app may be the one dispatching it.
        window.defer(cx, move |window, cx| {
            let _ = owner.update(cx, |app, cx| app.live_clicked(editor, at, block, false, window, cx));
        });
    }

    /// The picked block's range, while it's still there.
    fn picked_range(&self) -> Option<Range<usize>> {
        let start = self.picked?;
        self.page.blocks.iter().find(|(r, s)| r.start == start && matches!(s, Shape::Other)).map(|(r, _)| r.clone())
    }

    /// Remove the picked block with the blank line after it (or, last on the
    /// page, before it), and put the caret by where it was.
    fn remove_picked(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(range) = self.picked_range() else {
            self.picked = None;
            return false;
        };
        self.remove_block(range, forward, window, cx);
        true
    }

    /// Remove the Markdown `range` (a block, a list item) with the blank
    /// line after it (or, last on the page, before it), the caret going to
    /// the line after (`forward`) or before where it was.
    fn remove_block(&mut self, range: Range<usize>, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        // Blank lines only: the next line's indent stays.
        let after = self.source[range.end..].len() - self.source[range.end..].trim_start_matches(['\n', '\r']).len();
        let gone = if range.end + after < self.source.len() {
            range.start..range.end + after
        } else {
            let before = self.source[..range.start].len() - self.source[..range.start].trim_end_matches(['\n', '\r', ' ', '\t']).len();
            // The last block: the blank line before it goes; the note's
            // final newline stays.
            range.start - before..range.end
        };
        let previous = self.page.leaves.iter().rposition(|l| l.span.end <= range.start);
        // Removing a block undoes on its own, apart from any typing.
        self.last_edit = None;
        self.edit(gone.clone(), "", window, cx);
        self.last_edit = None;
        let next = self.page.leaves.iter().position(|l| l.span.start >= gone.start);
        let previous = previous.filter(|p| *p < self.page.leaves.len());
        let to = if forward { next.or(previous) } else { previous.or(next) };
        self.caret = Some(match to {
            Some(i) if Some(i) == next => self.page.leaves[i].flow.source_for(0),
            Some(i) => {
                let flow = &self.page.leaves[i].flow;
                flow.source_for(flow.text.len())
            }
            None => gone.start.min(self.source.len()),
        });
        cx.notify();
    }

    /// An arrow key with a block picked: the caret goes to the line before
    /// (or after) it. False when nothing is picked.
    fn leave_picked(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(range) = self.picked_range() else { return false };
        self.picked = None;
        self.caret = if forward {
            self.page.leaves.iter().find(|l| l.span.start >= range.end).map(|l| l.flow.source_for(0))
        } else {
            self.page.leaves.iter().rev().find(|l| l.span.end <= range.start).map(|l| l.flow.source_for(l.flow.text.len()))
        };
        cx.notify();
        true
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
    pub(crate) fn selected_text(&self) -> String {
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
        self.picked = None;
        // The raw block grows and shrinks with what's typed in it.
        if let Some(raw) = self.raw.clone() {
            let delta = text.len() as isize - range.len() as isize;
            let moved = |at: usize| (at as isize + delta).max(0) as usize;
            self.raw = if raw.start <= range.start && range.end <= raw.end {
                Some(raw.start..moved(raw.end))
            } else if range.end <= raw.start {
                Some(moved(raw.start)..moved(raw.end))
            } else if range.start >= raw.end {
                Some(raw)
            } else {
                None
            };
        }
        let text = text.to_string();
        self.own_edit = true;
        self.editor.update(cx, |editor, cx| {
            editor.set_selected_range(range.clone(), cx);
            editor.replace(text.clone(), window, cx);
        });
        self.refresh(cx);
        self.own_edit = false;
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
        // The block shown as Markdown stays so when what changes is in it.
        self.raw = self.raw.clone().and_then(|raw| moved_range(&self.source, &text, raw));
        self.own_edit = true;
        // Only what differs is replaced, so the editor (Split's) keeps its
        // place, and the caret goes where the change was, not to the top.
        let old = self.source.clone();
        let prefix = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count();
        let room = old.len().min(text.len()) - prefix;
        let suffix = old.bytes().rev().zip(text.bytes().rev()).take(room).take_while(|(a, b)| a == b).count();
        let mut start = prefix;
        while !old.is_char_boundary(start) || !text.is_char_boundary(start) {
            start -= 1;
        }
        let (old_end, new_end) = (old.len() - suffix, text.len() - suffix);
        let mut cut = 0;
        while !old.is_char_boundary(old_end + cut) || !text.is_char_boundary(new_end + cut) {
            cut += 1;
        }
        let middle = text[start..new_end + cut].to_string();
        let caret_at = caret.min(text.len());
        self.editor.update(cx, |editor, cx| {
            editor.set_selected_range(start..old_end + cut, cx);
            editor.replace(middle, window, cx);
            editor.set_selected_range(caret_at..caret_at, cx);
        });
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
                Kind::Quote { .. } => with_break(line(self.page.leaves[i].span.start)),
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
            // Out of the (innermost) quote: its `> ` goes. A list item's
            // later paragraph joins the one above, as text does.
            Kind::Quote { lead } if lead.contains('>') => {
                let line_start = start - lead.len();
                let mark = line_start + lead.rfind('>').unwrap_or(0);
                self.edit(mark..start, "", window, cx);
                self.caret = Some(mark);
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
        // An empty line above a heading or paragraph: that line goes, and
        // the block moves up to it.
        if matches!(leaf.kind, Kind::Text { .. }) {
            let block_start = match &leaf.kind {
                Kind::Text { block } => block.start,
                _ => start,
            };
            let before = &self.source[..block_start];
            let top_blank = block_start > 0 && before.chars().all(|c| c == '\n');
            if before.ends_with("\n\n\n") || top_blank {
                self.edit_keeping_caret(block_start - 1..block_start, "", window, cx);
                return;
            }
        }
        // Right after an image (or another block without text): pick it, so
        // the next Backspace removes it.
        let block = self.page.blocks.iter().position(|(r, _)| r.start <= start && start <= r.end);
        if let Some(b) = block.and_then(|b| b.checked_sub(1))
            && let (range, Shape::Other) = &self.page.blocks[b]
            && self.source[range.end..start].trim().is_empty()
            && matches!(leaf.kind, Kind::Text { .. })
        {
            self.picked = Some(range.start);
            self.picked_at = None;
            self.caret = None;
            self.anchor = None;
            cx.notify();
            return;
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
        if self.picked.is_some() && self.remove_picked(false, window, cx) {
            return;
        }
        if self.delete_selection(window, cx) {
            return;
        }
        // The last letter of a paragraph of its own: back to an empty line.
        if self.empty_line_again(false, window, cx) {
            return;
        }
        // An empty line after another: just this line goes.
        if let Some(Spot::Gap(at)) = self.spot()
            && at >= 2
            && self.source[..at].ends_with("\n\n")
            && self.source[at..].starts_with('\n')
            && quote_line(&self.source, at).is_none()
        {
            self.edit(at - 1..at, "", window, cx);
            self.caret = Some(at - 1);
            cx.notify();
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
        if self.picked.is_some() && self.remove_picked(true, window, cx) {
            return;
        }
        if self.delete_selection(window, cx) {
            return;
        }
        if self.empty_line_again(true, window, cx) {
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
        // An image (or other block) picked: a new empty line under it, the
        // caret on it.
        if let Some(range) = self.picked_range() {
            self.picked = None;
            self.edit(range.end..range.end, "\n", window, cx);
            return;
        }
        self.delete_selection(window, cx);
        if self.table_key("enter", window, cx) {
            return;
        }
        let (Some(caret), Some(spot), Some(flow)) = (self.caret, self.spot(), self.active_flow()) else { return };
        match spot {
            // In code, Enter is a new line of code.
            Spot::Leaf(i) if matches!(self.page.leaves[i].kind, Kind::Code { .. }) => {
                let Kind::Code { lead, .. } = &self.page.leaves[i].kind else { return };
                let lead = lead.clone();
                self.edit(caret..caret, &format!("\n{lead}"), window, cx);
                return;
            }
            // In a quote (or more of a list item), a new paragraph in it.
            Spot::Leaf(i) if matches!(self.page.leaves[i].kind, Kind::Quote { .. }) => {
                let Kind::Quote { lead } = &self.page.leaves[i].kind else { return };
                let between = format!("\n{}\n{lead}", lead.trim_end());
                let (at, text) = flow.split_at(flow.shown_for(caret));
                let text = text.replacen("\n\n", &between, 1);
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
        // On an empty line: one more empty line.
        if let Spot::Gap(at) = spot {
            self.edit(at..at, "\n", window, cx);
            return;
        }
        // At a heading's or paragraph's start: the whole block moves down a
        // line (its formatting with it), an empty line left above it.
        if let Spot::Leaf(i) = spot
            && let Kind::Text { block } = &self.page.leaves[i].kind
            && flow.shown_for(caret) == 0
            && !flow.text.is_empty()
        {
            let start = block.start;
            self.edit_keeping_caret(start..start, "\n", window, cx);
            return;
        }
        // At a paragraph's end: an empty line after it, to type on.
        if let Spot::Leaf(i) = spot
            && let Kind::Text { block } = &self.page.leaves[i].kind
            && flow.shown_for(caret) >= flow.text.len()
        {
            let end = block.end.max(self.page.leaves[i].span.end);
            if self.source[end..].is_empty() || self.source[end..].starts_with('\n') {
                self.edit(end..end, "\n", window, cx);
                return;
            }
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
        // A table cell is an input of its own: the arrows stay in it.
        if edge && self.in_cell() {
            return;
        }
        let to = if edge {
            self.next_stop(spot, forward).map(|stop| match stop {
                Spot::Leaf(l) => {
                    let leaf = &self.page.leaves[l];
                    if forward { leaf.flow.source_for(0) } else { leaf.flow.source_for(leaf.flow.text.len()) }
                }
                Spot::Gap(at) => at,
            })
        } else {
            // A letter on (a whole emoji), or past a unit such as math.
            let to = if forward {
                flow.step(at, true).max(next_boundary(&flow.text, at))
            } else {
                flow.step(at, false).min(previous_boundary(&flow.text, at))
            };
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

    /// Every empty line drawn on the page (each one a stop for the arrows).
    fn empty_lines(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut previous_end = 0;
        for (b, (range, _)) in self.page.blocks.iter().enumerate() {
            out.extend(blank_lines(&self.source, previous_end, range.start, b == 0));
            previous_end = range.end;
        }
        if !self.page.blocks.is_empty() {
            out.extend(blank_lines(&self.source, previous_end, self.source.len(), false));
        }
        out
    }

    /// Where an arrow past the edge of the caret's line goes: the next (or
    /// previous) line of text, or an empty line on the way to it.
    fn next_stop(&self, spot: Spot, forward: bool) -> Option<Spot> {
        let (start, end) = match spot {
            Spot::Leaf(i) => (self.page.leaves[i].span.start, self.page.leaves[i].span.end),
            Spot::Gap(at) => (at, at),
        };
        let leaf = self.neighbour(spot, forward);
        let empty = self.empty_lines().into_iter().filter(|&at| {
            if forward {
                at > end && leaf.is_none_or(|l| at < self.page.leaves[l].span.start)
            } else {
                at < start && leaf.is_none_or(|l| at > self.page.leaves[l].span.end)
            }
        });
        let empty = if forward { empty.min() } else { empty.max() };
        empty.map(Spot::Gap).or(leaf.map(Spot::Leaf))
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
        if self.in_cell() {
            return;
        }
        match self.next_stop(spot, down) {
            Some(Spot::Leaf(l)) => {
                let flow = &self.page.leaves[l].flow;
                self.caret = Some(flow.source_for(local.min(flow.text.len())));
                cx.notify();
            }
            Some(Spot::Gap(at)) => {
                self.caret = Some(at);
                cx.notify();
            }
            None => {}
        }
    }

    /// ⌘B, ⌘I, ⌘⇧X (and inline code from ⌘/): the mark on the selection (or the word at the
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
            Kind::Quote { lead } if lead.contains('>') => {
                let mark = start - lead.len() + lead.rfind('>').unwrap_or(0);
                self.edit_keeping_caret(mark..start, "", window, cx);
            }
            // More of a list item: quoted where it stands.
            Kind::Quote { .. } => self.edit_keeping_caret(start..start, "> ", window, cx),
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
        self.over_card = false;
        self.closing = None;
        self.mark_card = None;
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

    /// ⌘K on inline math: a card to edit its TeX, under it.
    fn open_math_card(&mut self, source: Range<usize>, tex: String, leaf: usize, window: &mut Window, cx: &mut Context<Self>) {
        let at = {
            let geometry = self.geometry.borrow();
            let flow = &self.page.leaves[leaf].flow;
            geometry.get(&leaf_key(leaf)).map(|g| {
                let top = g.position_for_index(flow.shown_for(source.start));
                point(top.x, top.y + g.line_height() + px(4.))
            })
        }
        .unwrap_or(point(px(120.), px(120.)));
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("TeX").default_value(tex));
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_math(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.math_card = Some(MathCard { source, input, at, _subscription: subscription });
        cx.notify();
    }

    /// Write the card's TeX back between the math's own `$`s (none: the
    /// math goes).
    fn apply_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.math_card.take() else { return };
        let tex = card.input.read(cx).value().trim().to_string();
        let dollars = self.source[card.source.clone()].chars().take_while(|c| *c == '$').count().max(1);
        let fence = "$".repeat(dollars);
        let markdown = if tex.is_empty() { String::new() } else { format!("{fence}{tex}{fence}") };
        self.focus_page(window, cx);
        self.edit(card.source, &markdown, window, cx);
    }

    fn close_math_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.math_card.take().is_some() {
            self.focus_page(window, cx);
            cx.notify();
        }
    }

    /// ⌘⇧E: the caret's block (or the picked one) as its Markdown, edited
    /// as text; again, drawn as before.
    fn toggle_markdown(&mut self, _: &ToggleMarkdown, _: &mut Window, cx: &mut Context<Self>) {
        if self.raw.is_some() {
            self.leave_raw(cx);
            return;
        }
        // Split shows the Markdown beside the page already.
        if self.in_split(cx) {
            return;
        }
        if !self.editor.read(cx).is_editable() {
            return;
        }
        let at = self.picked.or(self.caret);
        let Some(at) = at else { return };
        let Some((range, _)) = self.page.blocks.iter().find(|(r, _)| r.start <= at && at <= r.end).cloned() else { return };
        self.raw = Some(range.clone());
        self.page = Page::read_with_raw(&self.source, self.raw.clone());
        self.picked = None;
        self.caret = Some(if at < range.start || at > range.end { range.start } else { at });
        self.anchor = self.anchor.filter(|a| range.contains(a));
        self.link_card = None;
        self.math_card = None;
        self.table_card = None;
        cx.notify();
    }

    /// The raw block drawn again; the caret stays on the same Markdown, at
    /// the nearest place text can be typed.
    fn leave_raw(&mut self, cx: &mut Context<Self>) {
        let Some(raw) = self.raw.take() else { return };
        self.page = Page::read_with_raw(&self.source, None);
        self.anchor = None;
        if let Some(caret) = self.caret {
            self.caret = Some(self.nearest_caret(caret, raw));
        }
        cx.notify();
    }

    /// Where text can be typed nearest to `at` (Markdown), within `block`
    /// if it has any text.
    fn nearest_caret(&self, at: usize, block: Range<usize>) -> usize {
        if self.page.leaf_at(at).is_some() {
            return at;
        }
        let candidates = self.page.leaves.iter().filter(|l| l.span.start < block.end.max(block.start + 1) && block.start <= l.span.end);
        let best = candidates
            .flat_map(|l| [l.flow.source_for(0), l.flow.source_for(l.flow.text.len())])
            .min_by_key(|p| p.abs_diff(at));
        best.unwrap_or_else(|| {
            self.page.leaves.iter().map(|l| l.flow.source_for(0)).min_by_key(|p| p.abs_diff(at)).unwrap_or(at)
        })
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
        if self.selection().is_none()
            && let Some((source, tex)) = flow.math_at(flow.shown_for(caret))
        {
            self.open_math_card(source, tex, i, window, cx);
            return;
        }
        // In a table (not on a link): the table's card.
        if matches!(self.page.leaves[i].kind, Kind::Cell { .. }) && self.selection().is_none() {
            self.open_table_card(i, cx);
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

    /// The table starting at `start` as a grid of its cells' Markdown.
    fn grid(&self, start: usize) -> Option<Grid> {
        self.page.blocks.iter().find_map(|(range, shape)| match shape {
            Shape::Table { rows, aligns, .. } if range.start == start => Some(Grid {
                range: range.clone(),
                cells: rows
                    .iter()
                    .map(|row| row.iter().map(|i| self.source[self.page.leaves[*i].span.clone()].trim().to_string()).collect())
                    .collect(),
                aligns: aligns.clone(),
            }),
            _ => None,
        })
    }

    /// Change the shape of the table starting at `start`, around cell
    /// (`row`, `col`), and put the caret in the cell it lands on.
    fn table_op(&mut self, start: usize, row: usize, col: usize, op: TableOp, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut grid) = self.grid(start) else { return };
        window.focus(&self.focus, cx);
        let width = grid.cells.iter().map(Vec::len).max().unwrap_or(1);
        let empty_row = || vec![String::new(); width];
        let (mut row, mut col) = (row.min(grid.cells.len().saturating_sub(1)), col.min(width.saturating_sub(1)));
        match op {
            TableOp::RowAbove | TableOp::RowBelow => {
                // Never above the header.
                let at = if op == TableOp::RowAbove && row > 0 { row } else { row + 1 };
                grid.cells.insert(at, empty_row());
                row = at;
            }
            TableOp::AddRow => {
                grid.cells.push(empty_row());
                row = grid.cells.len() - 1;
                col = 0;
            }
            TableOp::DeleteRow => {
                if grid.cells.len() <= 1 {
                    return;
                }
                // The header goes: the next row becomes the header.
                grid.cells.remove(row);
                row = row.min(grid.cells.len() - 1);
            }
            TableOp::ColumnLeft | TableOp::ColumnRight | TableOp::AddColumn => {
                let at = match op {
                    TableOp::ColumnLeft => col,
                    TableOp::ColumnRight => col + 1,
                    _ => width,
                };
                for cells in &mut grid.cells {
                    cells.insert(at.min(cells.len()), String::new());
                }
                grid.aligns.insert(at.min(grid.aligns.len()), Align::None);
                col = at;
                if op == TableOp::AddColumn {
                    row = 0;
                }
            }
            TableOp::DeleteColumn => {
                if width <= 1 {
                    return;
                }
                for cells in &mut grid.cells {
                    if col < cells.len() {
                        cells.remove(col);
                    }
                }
                if col < grid.aligns.len() {
                    grid.aligns.remove(col);
                }
                col = col.min(width - 2);
            }
            TableOp::Align(align) => {
                while grid.aligns.len() <= col {
                    grid.aligns.push(Align::None);
                }
                grid.aligns[col] = align;
            }
        }
        let range = grid.range.clone();
        self.edit(range.clone(), &grid.markdown(), window, cx);
        self.table_card = None;
        let cell = self.page.leaves.iter().position(|l| {
            matches!(l.kind, Kind::Cell { row: r, col: c } if r == row && c == col) && l.span.start >= range.start
        });
        if let Some(cell) = cell {
            self.go_to_cell(cell, true, cx);
        }
    }

    /// The table card for the caret's cell, under it.
    fn open_table_card(&mut self, leaf: usize, cx: &mut Context<Self>) {
        let at = self
            .geometry
            .borrow()
            .get(&leaf_key(leaf))
            .map(|g| point(g.bounds().left(), g.bounds().bottom() + px(6.)))
            .unwrap_or(point(px(120.), px(120.)));
        self.table_card = Some((leaf, at));
        cx.notify();
    }

    /// Whether the caret is in a table cell.
    fn in_cell(&self) -> bool {
        matches!(self.spot(), Some(Spot::Leaf(i)) if matches!(self.page.leaves[i].kind, Kind::Cell { .. }))
    }

    /// ↓ on the page's last line (out of a code block, say): a new paragraph
    /// after everything.
    fn past_the_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.source.len();
        if !self.page.in_block(end) {
            self.caret = Some(end);
            cx.notify();
        } else if self.editor.read(cx).is_editable()
            && matches!(self.page.blocks.last(), Some((_, Shape::Code(_) | Shape::Quote(_) | Shape::List(_) | Shape::Table { .. } | Shape::Note { .. })))
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
        if self.action_menu.as_ref().is_some_and(|m| m.sub.is_some()) {
            self.close_sub(window, cx);
            return;
        }
        if self.action_menu.is_some() {
            self.close_actions(window, cx);
            return;
        }
        if self.math_card.is_some() {
            self.close_math_card(window, cx);
            return;
        }
        if self.mark_card.take().is_some() || self.text_color_card.take().is_some() {
            cx.notify();
            return;
        }
        if self.table_card.take().is_some() {
            cx.notify();
            return;
        }
        if self.picked.take().is_some() {
            cx.notify();
            return;
        }
        if self.raw.is_some() {
            self.leave_raw(cx);
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
        Self::flow_node_selected(flow, key, None, px(680.))
    }

    /// The same, with drawn text `selected` selected: its highlight (a
    /// `<mark>` colour) drops out there, or it would hide the selection.
    fn flow_node_selected(flow: &Flow, key: u64, selected: Option<&Range<usize>>, line: Pixels) -> BlockNode {
        let mut marks = Vec::new();
        for (range, mark) in &flow.marks {
            match selected.filter(|s| mark.highlight.is_some() && s.start < range.end && range.start < s.end) {
                Some(s) => {
                    let mut plain = mark.clone();
                    plain.highlight = None;
                    if range.start < s.start {
                        marks.push((range.start..s.start, mark.clone()));
                    }
                    marks.push((range.start.max(s.start)..range.end.min(s.end), plain));
                    if s.end < range.end {
                        marks.push((s.end..range.end, mark.clone()));
                    }
                }
                None => marks.push((range.clone(), mark.clone())),
            }
        }
        marks.retain(|(r, m)| !r.is_empty() && *m != gpui_kit::base::text::editor_host::TextMark::default());
        let mut paragraph = Paragraph::default();
        // Text, and images in it: each run of text with its own marks.
        let mut from = 0;
        let run = |paragraph: &mut Paragraph, range: Range<usize>| {
            if range.is_empty() {
                return;
            }
            let marks = marks
                .iter()
                .filter(|(r, _)| r.start < range.end && range.start < r.end)
                .map(|(r, m)| (r.start.max(range.start) - range.start..r.end.min(range.end) - range.start, m.clone()))
                .collect();
            paragraph.push(InlineNode::new(flow.text[range].to_string()).marks(marks));
        };
        // Pixels, or `50%` of the line (in pixels: a relative size would be
        // of the image's own box).
        let length = |value: &Option<String>| -> Option<DefiniteLength> {
            let value = value.as_deref()?.trim();
            match value.strip_suffix('%') {
                Some(percent) => percent.trim().parse::<f32>().ok().map(|p| (line * (p / 100.)).into()),
                None => value.trim_end_matches("px").parse::<f32>().ok().map(|p| px(p).min(line).into()),
            }
        };
        for picture in &flow.images {
            run(&mut paragraph, from..picture.shown.start);
            let mut image = gpui_kit::base::text::editor_host::ImageNode::default();
            image.url = picture.url.clone().into();
            image.alt = (!picture.alt.is_empty()).then(|| picture.alt.clone().into());
            image.width = length(&picture.width);
            image.height = length(&picture.height);
            // Its own size, at most the line's width (a photo is often far
            // wider).
            if image.width.is_none() && image.height.is_none() {
                image.width = Some(line.min(px(240.)).into());
            }
            paragraph.push(InlineNode::image(image));
            from = picture.shown.end;
        }
        run(&mut paragraph, from..flow.text.len());
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
        // The ⌘/ menu's search took the keys: the caret and selection stay,
        // drawn, so it shows what the menu works on.
        if self.was_focused && !focused && self.action_menu.is_none() && self.text_color_card.is_none() {
            if self.raw.is_some() {
                self.leave_raw(cx);
            }
            self.parked = self.caret.take().map(|caret| (caret, self.anchor.take(), self.revision));
            self.picked = None;
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
        // The picked block's ring: cleared here, set again below if still picked.
        if cx.try_global::<rich::PickedBlock>().and_then(|p| p.0.as_ref()).is_some_and(|(id, _)| *id == self.editor_id) {
            cx.set_global(rich::PickedBlock(None));
        }
        // The caret left the raw block (a click, an arrow): it's drawn again.
        if let Some(raw) = self.raw.clone()
            && self.caret.is_some_and(|c| c < raw.start || c > raw.end)
        {
            let caret = self.caret;
            self.raw = None;
            self.page = Page::read_with_raw(&self.source, None);
            self.caret = caret;
        }
        let spot = self.spot();
        let reading = typography::current(cx);
        let render_context = {
            let (owner, editor) = (self.owner.clone(), self.editor_id);
            let images = self.base.clone();
            RenderContext::from_app(cx)
                .with_body_font_size(px(reading.size as f32))
                // Images in the text, from beside the note as the reader finds them.
                .with_image_source(move |uri| {
                    let source = uri.to_string();
                    match &images {
                        Some(base) if !source.contains(':') => ImageSource::from(base.join(source)),
                        _ => ImageSource::from(uri.clone()),
                    }
                })
                .on_link_click(move |url, _, window, cx| {
                // Web addresses in the browser; notes (wiki links too) in enso.
                if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
                    cx.open_url(url);
                } else {
                    let url = url.to_string();
                    let _ = owner.update(cx, |app, cx| app.follow_note_link(editor, &url, window, cx));
                }
            })
        };
        // Text in its own colour (`<span style="color">`), line by line.
        let render_context = self.page.leaves.iter().enumerate().fold(render_context, |context, (i, leaf)| {
            if leaf.flow.colors.is_empty() {
                context
            } else {
                context.with_foreground_ranges(leaf_key(i), leaf.flow.colors.clone())
            }
        });
        let muted = cx.theme().muted_foreground;
        let weak = cx.entity().downgrade();
        let count = self.page.blocks.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut gap_placed = false;
        // The width of a line of text, for images in the text.
        let line_width = self.line_width();
        // What's selected, line by line (drawn ranges).
        let selected: HashMap<usize, Range<usize>> = self.selected_parts().into_iter().filter(|(_, r)| !r.is_empty()).collect();
        // A line of text: pressed, the caret goes there (and a drag selects).
        let line = |leaf: usize, first: bool, last: bool, rows_window: &mut Window, cx: &mut App| {
            let flow = &self.page.leaves[leaf].flow;
            let node = Self::flow_node_selected(flow, leaf_key(leaf), selected.get(&leaf), line_width);
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
        // Blank lines past the one between two blocks: empty lines, each one
        // there to click into and type on.
        // Drawn just as the line the caret types on (`gap`), so nothing
        // moves when the caret comes or goes.
        let blank = |at: usize, first: bool, rows_window: &mut Window, cx: &mut App| {
            let node = Self::flow_node(&Flow::empty(at), BLANK_LEAF + at as u64);
            let position = FlowPosition::at(&[&node], 0, false).first(first);
            let element = node.render_with_id(("live-blank", at), position, &render_context, rows_window, cx);
            let weak = weak.clone();
            div()
                .id(("live-blank-row", at))
                .w_full()
                .cursor_text()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    let _ = weak.update(cx, |view, cx| view.click_blank(at, window, cx));
                })
                .child(element)
                .into_any_element()
        };
        let mut previous_end = 0;
        let blocks = self.page.blocks.clone();
        for (b, (range, shape)) in blocks.clone().into_iter().enumerate() {
            for at in blank_lines(&self.source, previous_end, range.start, b == 0) {
                if spot == Some(Spot::Gap(at)) && !gap_placed {
                    rows.push(gap(at, rows.is_empty(), window, cx));
                    gap_placed = true;
                } else {
                    rows.push(blank(at, rows.is_empty(), window, cx));
                }
            }
            previous_end = range.end;
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
                // A footnote: its number, then its text.
                Shape::Note { leaf, number } => h_flex()
                    .w_full()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .h(px(reading.size as f32 * 1.618))
                            .min_w(px(18.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .text_color(muted)
                            .child(format!("{number}.")),
                    )
                    .child(line(leaf, first, last, window, cx))
                    .into_any_element(),
                Shape::List(items) => {
                    // One text line tall, the marker centred in it, so it sits
                    // beside the item's first line at any size.
                    let line_height = px(reading.size as f32 * 1.618);
                    let slot = || div().h(line_height).min_w(px(18.)).flex_shrink_0().flex().items_center().text_color(muted);
                    // Where an item's text starts, past its marker.
                    let text_at = |depth: usize| LIST_INDENT * depth as f32 + 26.;
                    let style = render_context.style();
                    let (bar, quoted_text) = (style.border(), style.muted_foreground());
                    let mut drawn: Vec<(Option<(usize, bool)>, AnyElement)> = Vec::new();
                    for row in items {
                        let leaf = row.leaf;
                        let content = match &self.page.leaves[leaf].kind {
                            Kind::Code { lang, .. } => {
                                let code = CodeBlock::from_code(self.page.leaves[leaf].flow.text.clone(), lang.clone()).with_leaf_key(leaf_key(leaf));
                                let node = BlockNode::CodeBlock(code);
                                let position = FlowPosition::at(&[&node], 0, true).first(true);
                                let element = node.render_with_id(("live-nested-code", leaf), position, &render_context, window, cx);
                                let weak = weak.clone();
                                div()
                                    .id(("live-nested-code-row", leaf))
                                    .flex_1()
                                    .min_w_0()
                                    .cursor_text()
                                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                        let _ = weak.update(cx, |view, cx| view.mouse_down(leaf, event, window, cx));
                                    })
                                    .child(element)
                                    .into_any_element()
                            }
                            _ => line(leaf, true, true, window, cx),
                        };
                        // Rows in a quote sit inside its bar: from there on.
                        let from = row.quote.map_or(0., |(depth, in_item)| if in_item { text_at(depth) } else { LIST_INDENT * depth as f32 });
                        let element = match row.marker {
                            Marker::Continue => div().w_full().flex().pl(px((text_at(row.depth) - from).max(0.))).child(content).into_any_element(),
                            Marker::Nothing => div().w_full().flex().child(content).into_any_element(),
                            marker => {
                                let marker = match marker {
                                    Marker::Number(n) => slot().child(format!("{n}.")).into_any_element(),
                                    Marker::Task(done) => {
                                        let weak = weak.clone();
                                        slot()
                                            .child(Checkbox::new(("live-task", leaf)).checked(done).on_click(move |_, window, cx| {
                                                let _ = weak.update(cx, |view, cx| view.toggle_task(leaf, window, cx));
                                            }))
                                            .into_any_element()
                                    }
                                    _ => slot().child("•").into_any_element(),
                                };
                                h_flex()
                                    .w_full()
                                    .items_start()
                                    .gap_2()
                                    .pl(px((LIST_INDENT * row.depth as f32 - from).max(0.)))
                                    .child(marker)
                                    .child(content)
                                    .into_any_element()
                            }
                        };
                        drawn.push((row.quote, element));
                    }
                    // Quoted rows in a run share one bar down their side.
                    let mut list = v_flex().w_full().gap_1().py_1();
                    let mut drawn = drawn.into_iter().peekable();
                    while let Some((quote, element)) = drawn.next() {
                        let Some((depth, in_item)) = quote else {
                            list = list.child(element);
                            continue;
                        };
                        let mut run = vec![element];
                        while drawn.peek().is_some_and(|(q, _)| *q == quote) {
                            run.push(drawn.next().map(|(_, e)| e).unwrap_or_else(|| div().into_any_element()));
                        }
                        let at = if in_item { text_at(depth) } else { LIST_INDENT * depth as f32 };
                        list = list.child(
                            div().w_full().pl(px(at)).child(
                                v_flex()
                                    .w_full()
                                    .gap_1()
                                    .border_l_2()
                                    .border_color(bar)
                                    .pl(px(reading.size as f32 * 0.9))
                                    .text_color(quoted_text)
                                    .children(run),
                            ),
                        );
                    }
                    list.into_any_element()
                }
                Shape::Quote(leaves) => {
                    let children = leaves.iter().map(|i| Self::flow_node_selected(&self.page.leaves[*i].flow, leaf_key(*i), selected.get(i), line_width)).collect();
                    let node = BlockNode::Blockquote { children, span: None };
                    let position = FlowPosition::at(&[&node], 0, last).first(first);
                    let element = node.render_with_id(("live-quote", b), position, &render_context, window, cx);
                    let weak = weak.clone();
                    div()
                        .id(("live-quote-row", b))
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
                    let start = range.start;
                    let editable = self.editor.read(cx).is_editable();
                    let style = render_context.style();
                    let (head, border) = (style.code_background(), style.border());
                    let theme = cx.theme().clone();
                    let width = cells.iter().map(Vec::len).max().unwrap_or(1);
                    // Columns share the width by how long their text runs.
                    let weights: Vec<f32> = (0..width)
                        .map(|c| {
                            let longest = cells.iter().filter_map(|r| r.get(c)).map(|i| self.page.leaves[*i].flow.text.chars().count()).max();
                            longest.unwrap_or(0).clamp(6, 40) as f32
                        })
                        .collect();
                    let mut frame = v_flex().w_full().rounded_md().border_1().border_color(border).overflow_hidden();
                    for (r, row) in cells.iter().enumerate() {
                        let mut line = h_flex().w_full().items_stretch().when(r + 1 < cells.len(), |l| l.border_b_1()).border_color(border);
                        if r == 0 {
                            line = line.bg(head).font_weight(FontWeight::SEMIBOLD);
                        }
                        for c in 0..width {
                            let align = aligns.get(c).copied().unwrap_or(Align::None);
                            let mut cell = div()
                                .id(ElementId::Name(format!("live-cell-{b}-{r}-{c}").into()))
                                .flex_basis(px(0.))
                                .flex_grow(weights[c])
                                .min_w(px(48.))
                                .relative()
                                .px_3()
                                .py(px(7.))
                                .when(c + 1 < width, |d| d.border_r_1())
                                .border_color(border)
                                .when(align == Align::Center, |d| d.text_center())
                                .when(align == Align::Right, |d| d.text_right());
                            let Some(&leaf) = row.get(c) else {
                                line = line.child(cell);
                                continue;
                            };
                            let node = Self::flow_node_selected(&self.page.leaves[leaf].flow, leaf_key(leaf), selected.get(&leaf), line_width);
                            let position = FlowPosition::at(&[&node], 0, true).first(true);
                            let text = node.render_with_id(("live-cell-text", leaf), position, &render_context, window, cx);
                            let weak = weak.clone();
                            // The whole cell takes the click, wherever its
                            // text sits in it.
                            cell = cell
                                .cursor_text()
                                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                    if *hovered {
                                        this.hovered_cell = Some(leaf);
                                    } else if this.hovered_cell == Some(leaf) {
                                        this.hovered_cell = None;
                                    }
                                    cx.notify();
                                }))
                                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                    let _ = weak.update(cx, |view, cx| view.mouse_down(leaf, event, window, cx));
                                })
                                .child(text);
                            // Hovered: the column's
                            // alignment, and the table's menu, at its top right.
                            if editable && self.hovered_cell == Some(leaf) {
                                let mut bar = h_flex()
                                    .absolute()
                                    .top(px(4.))
                                    .right(px(4.))
                                    .gap(px(2.))
                                    .p(px(2.))
                                    .toolbar_surface(&theme)
                                    .font_weight(FontWeight::NORMAL)
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
                                for (to, icon, label, id) in [
                                    (Align::Left, IconName::AlignHorizontalJustifyStart, "Align Left", "live-cell-left"),
                                    (Align::Center, IconName::AlignHorizontalJustifyCenter, "Align Center", "live-cell-center"),
                                    (Align::Right, IconName::AlignHorizontalJustifyEnd, "Align Right", "live-cell-right"),
                                ] {
                                    let current = align == to || (to == Align::Left && align == Align::None);
                                    bar = bar.child(
                                        crate::ui::icon_button((id, leaf), icon, label)
                                            .xsmall()
                                            .compact()
                                            .when(current, |b| b.bg(theme.accent))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.table_op(start, r, c, TableOp::Align(to), window, cx)
                                            })),
                                    );
                                }
                                bar = bar.child(
                                    crate::ui::icon_button(("live-cell-menu", leaf), IconName::Ellipsis, "Table Options")
                                        .xsmall()
                                        .compact()
                                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                            window.focus(&this.focus, cx);
                                            this.go_to_cell(leaf, true, cx);
                                            let at = event.position();
                                            this.table_card = Some((leaf, point(at.x - px(180.), at.y + px(12.))));
                                            cx.notify();
                                        })),
                                );
                                cell = cell.child(bar);
                            }
                            line = line.child(cell);
                        }
                        frame = frame.child(line);
                    }
                    let element = div().w_full().pb(px(reading.size as f32 * 0.9)).child(frame);
                    let group: SharedString = format!("live-table-{b}").into();
                    // Notion's + bars: one beside the table adds a column,
                    // one under it adds a row. They show while the pointer is
                    // over the table.
                    let bar = |id: &'static str, op: TableOp, label: &'static str| {
                        div()
                            .id((id, b))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .opacity(0.)
                            .cursor_pointer()
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.bg(theme.muted))
                            .group_hover(group.clone(), |s| s.opacity(1.))
                            .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| this.table_op(start, 0, 0, op, window, cx)))
                            .child(Icon::new(IconName::Plus).xsmall())
                    };
                    let table = div().flex_1().min_w_0().child(element);
                    v_flex()
                        .id(("live-table-row", b))
                        .group(group.clone())
                        .w_full()
                        .child(
                            h_flex()
                                .w_full()
                                .items_stretch()
                                .gap_1()
                                .child(table)
                                .when(editable, |row| row.child(bar("live-add-column", TableOp::AddColumn, "Add Column").w(px(16.)).my_2())),
                        )
                        .when(editable, |table| {
                            table.child(
                                h_flex()
                                    .w_full()
                                    .gap_1()
                                    .child(bar("live-add-row", TableOp::AddRow, "Add Row").flex_1().h(px(16.)).mt_neg_1())
                                    .child(div().w(px(16.))),
                            )
                        })
                        .into_any_element()
                }
                Shape::Code(leaf) => {
                    let kind = &self.page.leaves[leaf].kind;
                    let lang = match kind {
                        Kind::Code { lang, .. } => lang.clone(),
                        _ => None,
                    };
                    // A diagram, formula or HTML being edited as Markdown:
                    // drawn beside it as it's typed.
                    let preview_of = (self.raw.as_ref() == Some(&range))
                        .then(|| self.source[range.clone()].to_string())
                        .filter(|text| previews(text));
                    let code = CodeBlock::from_code(self.page.leaves[leaf].flow.text.clone(), lang).with_leaf_key(leaf_key(leaf));
                    let node = BlockNode::CodeBlock(code);
                    // Beside a preview, the gap after it goes under both.
                    let position = FlowPosition::at(&[&node], 0, last || preview_of.is_some()).first(first);
                    let element = node.render_with_id(("live-code", b), position, &render_context, window, cx);
                    let weak = weak.clone();
                    let theme = cx.theme().clone();
                    let preview = preview_of.map(|text| match mermaid_code(&text) {
                        Some(code) => rich::mermaid_preview(&code, self.owner.clone(), cx),
                        None => {
                            let view = TextView::markdown(("live-raw-preview", b), text).selectable(false).w_full();
                            let view = typography::style(view, cx);
                            let view = rich::decorate(view, self.owner.clone(), self.editor_id, self.base.clone(), range.start, 0);
                            // A formula or HTML on a panel like the Markdown's.
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .p_4()
                                .rounded(px(10.))
                                .border_1()
                                .border_color(theme.border.opacity(0.6))
                                .child(view)
                                .into_any_element()
                        }
                    });
                    let gap_after = render_context.style().paragraph_gap();
                    let code = div()
                        .id(("live-code-row", b))
                        .w_full()
                        .cursor_text()
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            let _ = weak.update(cx, |view, cx| view.mouse_down(leaf, event, window, cx));
                        })
                        .child(element);
                    match preview {
                        // Side by side: the Markdown on the left, what it draws
                        // on the right, each half the width.
                        Some(preview) => h_flex()
                            .w_full()
                            .items_stretch()
                            .gap_4()
                            .when(!last, |row| row.mb(gap_after))
                            .child(div().flex_1().min_w_0().child(code))
                            .child(div().flex_1().min_w_0().flex().child(preview))
                            .into_any_element(),
                        None => code.into_any_element(),
                    }
                }
                Shape::Other => {
                    let view = TextView::markdown(("live-view", b), self.source[range.clone()].to_string())
                        .selectable(false)
                        .w_full();
                    let view = typography::style(view, cx);
                    let base = self.base.clone();
                    let image_base = base.clone();
                    let weak = weak.clone();
                    let start = range.start;
                    let picked = focused && self.picked == Some(start);
                    // A picture rings itself; anything else rings its block.
                    let picture = {
                        let text = self.source[range.clone()].trim_start();
                        text.starts_with("<img") || text.starts_with("![")
                    };
                    let picked_range = (focused && self.picked == Some(start)).then(|| range.clone());
                    if let Some(range) = picked_range {
                        cx.set_global(rich::PickedBlock(Some((self.editor_id, range))));
                    }
                    let picked = picked && !picture;
                    let ring = cx.theme().ring;
                    let editable = self.editor.read(cx).is_editable();
                    // Edit: the block as its Markdown, drawn again under it
                    // as it's typed.
                    let edit: rich::DiagramEdit = {
                        let weak = weak.clone();
                        std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
                            let _ = weak.update(cx, |view, cx| view.edit_raw(start, window, cx));
                        })
                    };
                    let mermaid = mermaid_code(&self.source[range.clone()]);
                    let is_mermaid = mermaid.is_some();
                    let group: SharedString = format!("live-block-{b}").into();
                    let theme = cx.theme().clone();
                    let gap_after = render_context.style().paragraph_gap();
                    div()
                        .id(("live-block", b))
                        .group(group.clone())
                        .w_full()
                        .relative()
                        .on_click(move |event, window, cx| {
                            // A drag (an image's resize handle) isn't a click.
                            if let ClickEvent::Mouse(click) = event {
                                let moved = click.up.position - click.down.position;
                                if moved.x.abs() > px(4.) || moved.y.abs() > px(4.) {
                                    return;
                                }
                            }
                            let at = event.position();
                            let _ = weak.update(cx, |view, cx| view.click_other(start, Some(at), window, cx));
                        })
                        .when(picked, |block| {
                            block.child(div().absolute().top(px(-3.)).bottom(px(-3.)).left(px(-3.)).right(px(-3.)).rounded_md().border_2().border_color(ring))
                        })
                        // A diagram: drawn with an Edit button on its toolbar.
                        .when_some(mermaid.filter(|_| editable), |block, code| {
                            block.child(div().w_full().pb(gap_after).child(rich::mermaid_view(&code, self.owner.clone(), edit.clone(), cx)))
                        })
                        // Math, HTML…: an Edit button at the top right on hover.
                        .when(!is_mermaid && editable && !picture, |block| {
                            let edit = edit.clone();
                            block.child(
                                div()
                                    .absolute()
                                    .top(px(6.))
                                    .right(px(6.))
                                    .p(px(2.))
                                    .toolbar_surface(&theme)
                                    .opacity(0.)
                                    .group_hover(group.clone(), |bar| bar.opacity(1.))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        crate::ui::icon_button(("live-block-edit", b), IconName::Pencil, "Edit")
                                            .xsmall()
                                            .compact()
                                            .on_click(move |_, window, cx| {
                                                cx.stop_propagation();
                                                edit(window, cx);
                                            }),
                                    ),
                            )
                        })
                        .when(!is_mermaid || !editable, |block| block.child(
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
                        ))
                        .into_any_element()
                }
            };
            rows.push(element);
        }
        if !blocks.is_empty() {
            for at in blank_lines(&self.source, previous_end, self.source.len(), false) {
                if spot == Some(Spot::Gap(at)) && !gap_placed {
                    rows.push(gap(at, rows.is_empty(), window, cx));
                    gap_placed = true;
                } else {
                    rows.push(blank(at, rows.is_empty(), window, cx));
                }
            }
        }
        // A new paragraph after the last block (or in an empty note).
        if let Some(Spot::Gap(at)) = spot
            && !gap_placed
        {
            rows.push(gap(at, rows.is_empty(), window, cx));
        }
        // The notes linking here, at the end, as the reader lists them.
        let editor_id = self.editor_id;
        if let Ok(footer) = self.owner.update(cx, |app, cx| app.linked_from(editor_id, cx)) {
            rows.push(footer);
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
                        .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                            this.over_card = *hovered;
                            if *hovered {
                                this.closing = None;
                            } else {
                                this.close_hover_card_soon(window, cx);
                            }
                        }))
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
        // The table card closes once the caret leaves its cell.
        if let Some((leaf, _)) = self.table_card
            && self.spot() != Some(Spot::Leaf(leaf))
        {
            self.table_card = None;
        }
        let table_card = self.table_card.and_then(|(leaf, at)| {
            let Kind::Cell { row, col } = self.page.leaves.get(leaf)?.kind else { return None };
            let start = self.page.blocks.iter().find_map(|(range, shape)| match shape {
                Shape::Table { rows, .. } if rows.iter().flatten().any(|i| *i == leaf) => Some(range.start),
                _ => None,
            })?;
            let theme = cx.theme();
            let item = |id: &'static str, icon: Option<IconName>, label: &'static str, op: TableOp| {
                h_flex()
                    .id(id)
                    .h(px(26.))
                    .px_2()
                    .gap_2()
                    .items_center()
                    .rounded_md()
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent))
                    .child(
                        div()
                            .w(px(14.))
                            .flex()
                            .justify_center()
                            .text_color(theme.muted_foreground)
                            .children(icon.map(|icon| Icon::new(icon).xsmall())),
                    )
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| this.table_op(start, row, col, op, window, cx)))
            };
            let separator = || div().h_px().mx_1().my_0p5().bg(theme.border);
            Some(
                deferred(
                    anchored().position(at).snap_to_window_with_margin(px(8.)).child(
                        v_flex()
                            .id("live-table-card")
                            .occlude()
                            .w(px(200.))
                            .p_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .text_color(theme.popover_foreground)
                            .shadow_md()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(item("live-row-above", Some(IconName::ArrowUp), "Insert Row Above", TableOp::RowAbove))
                            .child(item("live-row-below", Some(IconName::ArrowDown), "Insert Row Below", TableOp::RowBelow))
                            .child(item("live-column-left", Some(IconName::ArrowLeft), "Insert Column Left", TableOp::ColumnLeft))
                            .child(item("live-column-right", Some(IconName::ArrowRight), "Insert Column Right", TableOp::ColumnRight))
                            .child(separator())
                            .child(item("live-align-left", None, "Align Left", TableOp::Align(Align::Left)))
                            .child(item("live-align-center", None, "Align Center", TableOp::Align(Align::Center)))
                            .child(item("live-align-right", None, "Align Right", TableOp::Align(Align::Right)))
                            .child(separator())
                            .child(item("live-delete-row", Some(IconName::Trash), "Delete Row", TableOp::DeleteRow))
                            .child(item("live-delete-column", Some(IconName::Trash), "Delete Column", TableOp::DeleteColumn)),
                    ),
                )
                .with_priority(1),
            )
        });
        // A highlight card opened with ⌘K goes once the caret leaves it.
        if let Some(card) = &self.mark_card
            && !card.hover
            && !card.pinned
            && !self.caret.is_some_and(|c| card.mark.source.start <= c && c <= card.mark.source.end)
        {
            self.mark_card = None;
        }
        // A colour card's frame and its picker's slot: the highlight card's
        // and the text colour card's are these same ones.
        let swatch_card = |id: &'static str, theme: &gpui_kit::component::Theme| {
            h_flex()
                .id(id)
                .occlude()
                .p_1p5()
                .gap_1p5()
                .items_center()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_md()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let text_color_card = self.text_color_card.as_ref().map(|card| {
            let theme = cx.theme().clone();
            let current = card.current.map(rich::highlight_hex);
            let swatches = actions::TEXT_COLORS.iter().enumerate().map(|(i, (name, value))| {
                let swatch: Hsla = rgb(*value).into();
                let selected = current.as_deref() == Some(rich::highlight_hex(swatch).as_str());
                let name: SharedString = format!("{name} Text").into();
                div()
                    .id(("live-text-color", i))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if selected { theme.foreground.opacity(0.45) } else { theme.border })
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(swatch)
                    .cursor_pointer()
                    .hover(|tile| tile.bg(theme.secondary))
                    .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx))
                    .child("A")
                    .on_click(cx.listener(move |this, _, window, cx| this.choose_text_color(Some(swatch), window, cx)))
            });
            deferred(
                anchored().position(card.at).snap_to_window_with_margin(px(8.)).child(
                    swatch_card("live-text-color-card", &theme)
                        .children(swatches)
                        .child(
                            ColorPicker::new(&card.picker)
                                .icon(IconName::Palette)
                                .xsmall()
                                .accessibility_label("Custom text color"),
                        )
                        .child(div().w_px().h(px(18.)).bg(theme.border))
                        .child(
                            crate::ui::icon_button("live-text-color-remove", IconName::CircleX, "Default Text")
                                .xsmall()
                                .compact()
                                .on_click(cx.listener(|this, _, window, cx| this.choose_text_color(None, window, cx))),
                        ),
                ),
            )
            .with_priority(1)
        });
        let mark_card = self.mark_card.as_ref().map(|card| {
            let theme = cx.theme();
            let current = rich::highlight_hex(card.mark.color);
            let swatches = rich::HIGHLIGHTS.iter().enumerate().map(|(i, (name, value))| {
                let swatch: Hsla = rgb(*value).into();
                let selected = rich::highlight_hex(swatch) == current;
                let name: SharedString = (*name).into();
                div()
                    .id(("live-mark-color", i))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .bg(swatch)
                    .border_1()
                    .border_color(if selected { theme.foreground.opacity(0.45) } else { swatch.blend(black().opacity(0.08)) })
                    .text_color(rgb(0x303235))
                    .cursor_pointer()
                    .hover(move |tile| tile.bg(swatch.blend(black().opacity(0.06))))
                    .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx))
                    .when(selected, |tile| tile.child(Icon::new(IconName::Check).size(px(12.))))
                    .on_click(cx.listener(move |this, _, window, cx| this.recolor_mark(Some(swatch), window, cx)))
            });
            deferred(
                anchored().position(card.at).snap_to_window_with_margin(px(8.)).child(
                    swatch_card("live-mark-card", theme)
                        .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                            this.over_card = *hovered;
                            if *hovered {
                                this.closing = None;
                            } else {
                                this.close_hover_card_soon(window, cx);
                            }
                        }))
                        .children(swatches)
                        .child(
                            div()
                                .id("live-mark-picker")
                                .capture_any_mouse_down(cx.listener(|this, _, _, _| {
                                    if let Some(card) = this.mark_card.as_mut() {
                                        card.pinned = true;
                                        card.hover = false;
                                    }
                                }))
                                .child(
                                    ColorPicker::new(&card.picker)
                                        .icon(IconName::Palette)
                                        .xsmall()
                                        .accessibility_label("Custom highlight color"),
                                ),
                        )
                        .child(div().w_px().h(px(18.)).bg(theme.border))
                        .child(
                            crate::ui::icon_button("live-mark-remove", IconName::CircleX, "Remove Highlight")
                                .xsmall()
                                .compact()
                                .on_click(cx.listener(|this, _, window, cx| this.recolor_mark(None, window, cx))),
                        ),
                ),
            )
            .with_priority(1)
        });
        // The menu follows what changes while it's open (⌘B on the
        // selection, say): its buttons, its list, and where it works.
        if self.action_menu.is_some() && (self.caret.is_some() || self.picked.is_some()) {
            let (buttons, actions) = (self.buttons(), self.actions(self.in_split(cx)));
            let scope = actions::Scope { caret: self.caret, anchor: self.anchor, picked: self.picked };
            if let Some(menu) = self.action_menu.as_mut() {
                menu.buttons = buttons;
                if !actions.is_empty() {
                    menu.actions = actions;
                }
                menu.scope = scope;
            }
        }
        let action_menu = self.action_menu.as_ref().map(|menu| {
            let theme = cx.theme().clone();
            let mut command = Command::new(&menu.state)
                .placeholder("Actions…")
                .max_h(px(320.))
                .scroll_bounce(false)
                .rounded(crate::ui::MODAL_RADIUS - px(1.))
                .bordered(false)
                .bg(gpui_kit::transparent_black());
            // What each row stands for, so a choice finds it again.
            #[derive(Clone, Copy)]
            enum Row {
                Act(usize),
                Sub(&'static str),
            }
            // A row of the menu (`main`: hovering it puts a submenu away) or
            // of a submenu.
            let hover_weak = cx.entity().downgrade();
            let item = |action: &actions::Action, group: &'static str, main: bool, n: usize| {
                let label = action.label.clone();
                let swatch = action.swatch;
                let letter = action.letter;
                let ink = theme.foreground;
                let border = theme.border;
                let weak = hover_weak.clone();
                CommandItem::new()
                    .label(label.clone())
                    .keywords(action.keywords.iter().copied().chain(std::iter::once(group)))
                    .child(move |_, _| {
                        let weak = weak.clone();
                        h_flex()
                            .id(("live-action-row", n))
                            .w_full()
                            .gap_2()
                            .items_center()
                            .when(main, |row| {
                                row.on_hover(move |hovered, window, cx| {
                                    if *hovered {
                                        let _ = weak.update(cx, |this, cx| this.close_sub(window, cx));
                                    }
                                })
                            })
                            .when_some(swatch.filter(|_| !letter), |row, swatch| {
                                row.child(div().size(px(14.)).rounded(px(4.)).bg(swatch).border_1().border_color(border))
                            })
                            // A text colour: an "A" in it, the page's own for Default.
                            .when_some(swatch.filter(|_| letter), |row, swatch| {
                                let color = if swatch.a == 0. { ink } else { swatch };
                                row.child(
                                    div()
                                        .size(px(18.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(border)
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(color)
                                        .child("A"),
                                )
                            })
                            .child(label.clone())
                    })
            };
            let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
            for (i, action) in menu.actions.iter().enumerate() {
                match groups.iter_mut().find(|(g, _)| *g == action.group) {
                    Some((_, rows)) => rows.push(i),
                    None => groups.push((action.group, vec![i])),
                }
            }
            // Searching finds every action; otherwise Convert To, Color and
            // Case are rows that open their own menu beside this one.
            let searching = !menu.state.read(cx).query(cx).trim().is_empty();
            let mut sections: Vec<(&'static str, Vec<Row>)> = Vec::new();
            if searching {
                sections = groups.iter().map(|(g, rows)| (*g, rows.iter().map(|i| Row::Act(*i)).collect())).collect();
            } else {
                let subs: Vec<Row> =
                    groups.iter().filter(|(g, _)| actions::SUBMENUS.contains(g)).map(|(g, _)| Row::Sub(*g)).collect();
                if !subs.is_empty() {
                    sections.push(("Text", subs));
                }
                for (g, rows) in groups.iter().filter(|(g, _)| !actions::SUBMENUS.contains(g)) {
                    sections.push((*g, rows.iter().map(|i| Row::Act(*i)).collect()));
                }
            }
            let open_sub = menu.sub.as_ref().map(|(name, _)| *name);
            for (name, rows) in &sections {
                let items: Vec<CommandItem> = rows
                    .iter()
                    .map(|row| match *row {
                        Row::Act(i) => item(&menu.actions[i], name, true, i),
                        Row::Sub(sub) => {
                            let muted = theme.muted_foreground;
                            let lit = open_sub == Some(sub);
                            let accent = theme.accent;
                            let weak = hover_weak.clone();
                            CommandItem::new().label(sub).child(move |_, _| {
                                let weak = weak.clone();
                                // Resting on it opens it (the menu keeps the keys).
                                h_flex()
                                    .id(SharedString::from(format!("live-sub-{sub}")))
                                    .on_hover(move |hovered, window, cx| {
                                        if *hovered {
                                            let _ = weak.update(cx, |this, cx| this.show_sub(sub, false, window, cx));
                                        }
                                    })
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .when(lit, |row| row.bg(accent).rounded_sm())
                                    .child(sub)
                                    .child(Icon::new(IconName::ChevronRight).size_3p5().text_color(muted))
                            })
                        }
                    })
                    .collect();
                command = command.group(CommandGroup::new().label(*name).items(items));
            }
            let order: Vec<Vec<Row>> = sections.into_iter().map(|(_, rows)| rows).collect();
            let weak = cx.entity().downgrade();
            let cancel = weak.clone();
            let command = command
                .on_confirm(move |index, window, cx| {
                    match order.get(index.section).and_then(|rows| rows.get(index.row)).copied() {
                        Some(Row::Act(i)) => {
                            let _ = weak.update(cx, |this, cx| this.choose_action(i, window, cx));
                        }
                        Some(Row::Sub(name)) => {
                            let _ = weak.update(cx, |this, cx| this.open_sub(name, window, cx));
                        }
                        None => {}
                    }
                })
                // After this key's dispatch: Escape also reaches the page's
                // Stop Editing, which closes the menu itself and must find it
                // open (else it would put the caret away too).
                .on_cancel(move |window, cx| {
                    let cancel = cancel.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = cancel.update(cx, |this, cx| this.close_actions(window, cx));
                    });
                });
            // The open submenu: its group's actions.
            let sub = menu.sub.as_ref().map(|(name, state)| {
                let rows = groups.iter().find(|(g, _)| g == name).map(|(_, rows)| rows.clone()).unwrap_or_default();
                // Its sections (Text Color, Background Color), in order.
                let mut sections: Vec<(&'static str, Vec<usize>)> = Vec::new();
                for &i in &rows {
                    let title = menu.actions[i].section.unwrap_or(name);
                    match sections.iter_mut().find(|(t, _)| *t == title) {
                        Some((_, rows)) => rows.push(i),
                        None => sections.push((title, vec![i])),
                    }
                }
                let weak = cx.entity().downgrade();
                let back = weak.clone();
                let mut command = Command::new(state)
                    .placeholder(SharedString::from(format!("{name}…")))
                    .max_h(px(420.))
                    .scroll_bounce(false)
                    .rounded(crate::ui::MODAL_RADIUS - px(1.))
                    .bordered(false)
                    .bg(gpui_kit::transparent_black());
                for (title, rows) in &sections {
                    let items: Vec<CommandItem> = rows.iter().map(|i| item(&menu.actions[*i], name, false, 1000 + *i)).collect();
                    command = command.group(CommandGroup::new().label(*title).items(items));
                }
                let order: Vec<Vec<usize>> = sections.into_iter().map(|(_, rows)| rows).collect();
                let command = command
                    .on_confirm(move |index, window, cx| {
                        if let Some(&i) = order.get(index.section).and_then(|rows| rows.get(index.row)) {
                            let _ = weak.update(cx, |this, cx| this.choose_action(i, window, cx));
                        }
                    })
                    .on_cancel(move |window, cx| {
                        let back = back.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = back.update(cx, |this, cx| this.close_sub(window, cx));
                        });
                    });
                div()
                    .id("live-actions-sub")
                    .occlude()
                    .w(px(240.))
                    .modal_surface(&theme)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        // Not a press outside the menu: the menu stays.
                        this.sub_pressed = true;
                        cx.stop_propagation();
                    }))
                    .child(div().rounded(crate::ui::MODAL_RADIUS - px(1.)).overflow_hidden().child(command))
            });
            // The submenu is placed on its own beside the menu (to its left
            // near the window's right edge), so fitting it in the window
            // never moves the menu under the pointer.
            let sub = sub.map(|sub| {
                let room = window.viewport_size().width;
                let x = if menu.at.x + px(264. + 240. + 8.) > room { menu.at.x - px(244.) } else { menu.at.x + px(264.) };
                deferred(anchored().position(point(x, menu.at.y)).snap_to_window_with_margin(px(8.)).child(sub))
                    .with_priority(2)
                    .into_any_element()
            });
            let main = deferred(
                anchored().position(menu.at).snap_to_window_with_margin(px(8.)).child(
                    h_flex()
                        .items_start()
                        .gap_1()
                        // A press outside the menu (and its submenu) closes it.
                        .on_mouse_down_out(cx.listener(|_, _, window, cx| {
                            let view = cx.entity().downgrade();
                            window.defer(cx, move |window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    if !std::mem::take(&mut this.sub_pressed) {
                                        this.close_actions(window, cx);
                                    }
                                });
                            });
                        }))
                        .child(
                    div()
                        .id("live-actions")
                        .debug_selector(|| "live-actions".into())
                        .occlude()
                        .w(px(260.))
                        .modal_surface(&theme)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(!menu.buttons.is_empty(), |surface| {
                            let buttons = menu.buttons.iter().map(|b| {
                                let act = b.act.clone();
                                crate::ui::icon_button(SharedString::from(format!("live-format-{}", b.label)), b.icon, b.label)
                                    .small()
                                    .when(b.active, |button| button.bg(theme.accent))
                                    .on_click(cx.listener(move |this, _, window, cx| this.press_button(act.clone(), window, cx)))
                            });
                            surface.child(
                                h_flex()
                                    .w_full()
                                    .px_1p5()
                                    .pt_1p5()
                                    .pb_1()
                                    .gap_0p5()
                                    .justify_between()
                                    .children(buttons),
                            )
                            .child(div().h_px().mx_1p5().bg(theme.border))
                        })
                        .child(div().rounded(crate::ui::MODAL_RADIUS - px(1.)).overflow_hidden().child(command)),
                        ),
                ),
            )
            .with_priority(2)
            .into_any_element();
            std::iter::once(main).chain(sub).collect::<Vec<_>>()
        });
        // The hovered (or resized) image in the text: a handle on each side,
        // as images on their own line have. Placed in the page, so it scrolls
        // with it, from where the image was last drawn.
        let image_handles = self
            .image_drag
            .as_ref()
            .map(|d| d.image.clone())
            .and_then(|image| {
                let leaf = self.page.leaves.iter().position(|l| l.flow.images.iter().any(|i| i.source == image.source))?;
                Some((leaf, image))
            })
            .or_else(|| self.hovered_image.clone())
            .filter(|_| self.editor.read(cx).is_editable())
            .and_then(|(leaf, image)| Some((image.clone(), self.image_rect(leaf, &image)?)))
            .map(|(image, rect)| {
                let view = self.scroll.bounds();
                let offset = self.scroll.offset();
                let left = rect.left() - view.left() - offset.x;
                let top = rect.top() - view.top() - offset.y;
                // The same handles as an image on its own line.
                let handle = |id: &'static str, side_left: bool| {
                    let image = image.clone();
                    let width = rect.size.width;
                    rich::resize_handle(div().id(id), side_left)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_image_drag(image.clone(), width, side_left, event.position.x, cx);
                        }))
                };
                // The same toolbar as an image on its own line, less its
                // alignment (it's in a line of text).
                let theme = cx.theme().clone();
                let edit: rich::DiagramEdit = {
                    let (owner, editor) = (self.owner.clone(), self.editor_id);
                    let expected = self.source.get(image.source.clone()).unwrap_or_default().to_string();
                    let picture = rich::Picture::inline(&image.url, &image.alt, image.width.as_deref(), image.height.as_deref());
                    let range = image.source.clone();
                    std::rc::Rc::new(move |w: &mut Window, cx: &mut App| {
                        let _ = owner.update(cx, |app, cx| app.open_picture(editor, range.clone(), expected.clone(), picture.clone(), w, cx));
                    })
                };
                let (toolbar, _) = rich::image_tools(
                    rich::image_toolbar_frame(&theme),
                    &image.url,
                    self.base.as_deref(),
                    self.owner.clone(),
                    &theme,
                    false,
                    Some(edit),
                );
                div()
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(rect.size.width)
                    .h(rect.size.height)
                    // The handles over the toolbar, so they're always grabbed.
                    .child(toolbar)
                    .child(handle("live-image-handle-left", true))
                    .child(handle("live-image-handle-right", false))
            });
        let math_card = self.math_card.as_ref().map(|card| {
            let theme = cx.theme();
            deferred(
                anchored().position(card.at).snap_to_window_with_margin(px(8.)).child(
                    h_flex()
                        .id("live-math-card")
                        .occlude()
                        .w(px(320.))
                        .p_1p5()
                        .gap_1()
                        .items_center()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .shadow_md()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.close_math_card(window, cx);
                            }
                        }))
                        .child(div().flex_1().min_w_0().child(Input::new(&card.input).small()))
                        .child(
                            crate::ui::icon_button("live-math-apply", IconName::Check, "Apply")
                                .xsmall()
                                .compact()
                                .on_click(cx.listener(|this, _, window, cx| this.apply_math(window, cx))),
                        ),
                ),
            )
            .with_priority(1)
        });
        let end_room = (window.viewport_size().height * 0.4).max(px(120.));
        let content = v_flex()
            .id("live-page")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::toggle_markdown))
            .on_action(cx.listener(Self::open_actions))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::soft_break))
            .on_action(cx.listener(|this, _: &Left, _, cx| {
                if this.leave_picked(false, cx) { return }
                if !this.collapse(false, cx) { this.moving(false, cx, |t, cx| t.horizontal(false, cx)) }
            }))
            .on_action(cx.listener(|this, _: &Right, _, cx| {
                if this.leave_picked(true, cx) { return }
                if !this.collapse(true, cx) { this.moving(false, cx, |t, cx| t.horizontal(true, cx)) }
            }))
            .on_action(cx.listener(|this, _: &Up, _, cx| {
                if !this.leave_picked(false, cx) { this.moving(false, cx, |t, cx| t.vertical(false, cx)) }
            }))
            .on_action(cx.listener(|this, _: &Down, w, cx| {
                if this.leave_picked(true, cx) { return }
                let before = this.caret;
                this.moving(false, cx, |t, cx| t.vertical(true, cx));
                if this.caret == before && !this.in_cell() {
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
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.mouse_up(w, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, w, cx| this.mouse_up(w, cx)))
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
            .on_action(cx.listener(|this, _: &Bold, w, cx| this.toggle_format(Wrap::Strong, w, cx)))
            .on_action(cx.listener(|this, _: &Italic, w, cx| this.toggle_format(Wrap::Emphasis, w, cx)))
            .on_action(cx.listener(|this, _: &Strike, w, cx| this.toggle_format(Wrap::Delete, w, cx)))
            .on_action(cx.listener(|this, _: &InlineCode, w, cx| this.toggle_format(Wrap::Code, w, cx)))
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
            .children(table_card)
            .children(math_card)
            .children(mark_card)
            .children(text_color_card)
            .children(image_handles)
            .children(action_menu.into_iter().flatten())
            // Below the last block: a click there writes a new paragraph at the
            // end. It's tall (part of the window), so the page can scroll past
            // its end: what shrinks near the end (an image resized) doesn't
            // pull the page up with it.
            .child(div().id("live-end").w_full().min_h(end_room).cursor_text().on_click(cx.listener(
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
        // Find's matches, softly, the current one strongly.
        if !self.found.is_empty() {
            let (soft, strong) = crate::find::colors(cx);
            for (n, found) in self.found.iter().enumerate() {
                let Some(i) = self.page.leaf_at(found.start) else { continue };
                let flow = &self.page.leaves[i].flow;
                let shown = flow.shown_for(found.start)..flow.shown_for(found.end);
                let color = if Some(n) == self.found_current { strong } else { soft };
                decorations.entry(leaf_key(i)).or_default().push((shown, color));
            }
        }
        if let Some(selection) = self.selection().filter(|_| self.shows_selection(focused)) {
            let color = cx.theme().selection;
            for (i, leaf) in self.page.leaves.iter().enumerate() {
                let (start, end) = (selection.start.max(leaf.span.start), selection.end.min(leaf.span.end));
                if start > end || (start == end && !(selection.start < leaf.span.start && leaf.span.end < selection.end)) {
                    continue;
                }
                let shown = leaf.flow.shown_for(start)..leaf.flow.shown_for(end);
                if !shown.is_empty() {
                    decorations.entry(leaf_key(i)).or_default().push((shown, color));
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
        let typed = range.is_none();
        // An empty code block: its closing fence starts where the code would.
        // What's typed goes on a line of its own before it.
        let text = match self.spot() {
            Some(Spot::Leaf(i))
                if matches!(self.page.leaves[i].kind, Kind::Code { .. })
                    && self.page.leaves[i].span.is_empty()
                    && !text.ends_with('\n') =>
            {
                let lead = match &self.page.leaves[i].kind {
                    Kind::Code { lead, .. } => lead.clone(),
                    _ => String::new(),
                };
                let at = self.page.leaves[i].span.start;
                let fence = self.source[at..].trim_start_matches(' ');
                if fence.starts_with("```") || fence.starts_with("~~~") {
                    let caret = at + text.len();
                    self.edit(at..at, &format!("{text}\n{lead}"), window, cx);
                    self.caret = Some(caret);
                    cx.notify();
                    return;
                }
                text
            }
            _ => text,
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
        // On an empty line: a paragraph of its own, apart from the blocks
        // around it.
        if typed
            && let Some(Spot::Gap(at)) = self.spot()
            && quote_line(&self.source, at).is_none()
        {
            let before = &self.source[..at];
            let after = &self.source[at..];
            // Empty lines above stay empty lines: past them, the one blank
            // line Markdown needs (none at the very top).
            let top = self.page.blocks.first().is_none_or(|(r, _)| at < r.start);
            let from = self.page.blocks.iter().rev().find(|(r, _)| r.end <= at).map_or(0, |(r, _)| r.end);
            let to = self.page.blocks.iter().find(|(r, _)| r.start > at).map_or(self.source.len(), |(r, _)| r.start);
            let rows_above = blank_lines(&self.source, from, to, top).into_iter().filter(|r| *r < at).count();
            let blank_above = before.len() - before.trim_end_matches('\n').len();
            let blank_above = blank_above.saturating_sub(1);
            let need = if top { rows_above } else { rows_above + 1 };
            let prefix = if self.page.in_block(at) {
                // A blank line inside a block (a list just left): apart from
                // what's above, no more.
                if before.ends_with("\n\n") { String::new() } else if before.ends_with('\n') { "\n".into() } else { "\n\n".into() }
            } else if !before.ends_with('\n') && at > 0 {
                "\n".repeat(need + 1)
            } else {
                "\n".repeat(need.saturating_sub(blank_above))
            };
            let prefix = prefix.as_str();
            let suffix = if after.is_empty() || after.starts_with("\n\n") { "" } else if after.starts_with('\n') { "\n" } else { "\n\n" };
            self.edit(at..at, &format!("{prefix}{text}{suffix}"), window, cx);
            self.caret = Some(at + prefix.len() + text.len());
            cx.notify();
            return;
        }
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

/// Where `range` of `old` is in `new`, when what changed between them is
/// within it or wholly before or after it.
fn moved_range(old: &str, new: &str, range: Range<usize>) -> Option<Range<usize>> {
    let prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old.bytes().rev().zip(new.bytes().rev()).take(room).take_while(|(a, b)| a == b).count();
    let (changed_end, delta) = (old.len() - suffix, new.len() as isize - old.len() as isize);
    let shift = |at: usize| (at as isize + delta) as usize;
    let moved = if range.start <= prefix && changed_end <= range.end {
        range.start..shift(range.end)
    } else if changed_end <= range.start {
        shift(range.start)..shift(range.end)
    } else if prefix >= range.end {
        range
    } else {
        return None;
    };
    (moved.end <= new.len() && new.is_char_boundary(moved.start) && new.is_char_boundary(moved.end)).then_some(moved)
}

/// Whether a block shown as its Markdown is drawn under it too: one that
/// reads nothing like its Markdown (a diagram, a formula, HTML).
fn previews(block: &str) -> bool {
    let text = block.trim_start();
    mermaid_code(text).is_some() || text.starts_with("> [!") || text.starts_with("$$") || text.starts_with("\\[") || text.starts_with('<')
}

/// A ```mermaid fence's diagram source.
fn mermaid_code(block: &str) -> Option<String> {
    let (first, rest) = block.split_once('\n')?;
    let fence = first.trim_start();
    let lang = fence.trim_start_matches(['`', '~']).trim();
    if !(fence.starts_with("```") || fence.starts_with("~~~")) || !crate::diagram::is_mermaid(Some(lang)) {
        return None;
    }
    let body = rest.trim_end();
    let body = body.strip_suffix("```").or_else(|| body.strip_suffix("~~~")).unwrap_or(body);
    Some(body.trim_end_matches('\n').to_string() + "\n")
}

/// The empty lines drawn between `from` (a block's end) and `to` (the next
/// block's start, or the note's end): where each starts. Between two
/// blocks one blank line is the gap Markdown needs, so it isn't one of
/// them; at the very top every blank line is.
fn blank_lines(source: &str, from: usize, to: usize, top: bool) -> Vec<usize> {
    let mut starts: Vec<usize> = Vec::new();
    if top && from == 0 && to > 0 {
        starts.push(0);
    }
    for (i, c) in source[from..to].char_indices() {
        if c == '\n' && from + i + 1 < to {
            starts.push(from + i + 1);
        }
    }
    // Only blank lines count (a block's own last line can end in spaces).
    starts.retain(|&at| source[at..].split('\n').next().is_some_and(|line| line.trim().is_empty()));
    if !top {
        starts.pop();
    }
    starts
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

    #[test]
    fn diagrams_and_formulas_preview_while_edited() {
        let diagram = "```mermaid\nflowchart LR\n  a --> b\n```";
        assert_eq!(super::mermaid_code(diagram).as_deref(), Some("flowchart LR\n  a --> b\n"));
        assert_eq!(super::mermaid_code("```rust\nfn a() {}\n```"), None);
        assert!(super::previews(diagram) && super::previews("$$\nx^2\n$$") && super::previews("<details>\nmore\n</details>"));
        assert!(!super::previews("Plain **text**") && !super::previews("- item"));
        assert!(super::previews("> [!TIP]\n> Use it."));
    }

    #[test]
    fn undo_keeps_the_block_being_edited() {
        let old = "A\n\n$$\nx^2\n$$\n\nB";
        let raw = 3..12;
        assert_eq!(&old[raw.clone()], "$$\nx^2\n$$");
        // Typed inside it: it grows and shrinks with the text.
        assert_eq!(super::moved_range(old, "A\n\n$$\nx^23\n$$\n\nB", raw.clone()), Some(3..13));
        // Before it: it moves; after it: it stays.
        assert_eq!(super::moved_range(old, "AA\n\n$$\nx^2\n$$\n\nB", raw.clone()), Some(4..13));
        assert_eq!(super::moved_range(old, "A\n\n$$\nx^2\n$$\n\nBB", raw.clone()), Some(3..12));
        // Across its edge: gone.
        assert_eq!(super::moved_range(old, "A\n\nB", raw), None);
    }
}
