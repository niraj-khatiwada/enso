use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc, sync::Arc, time::Duration};

use gpui_kit::base::input::Rope;
use gpui_kit::base::text::editor_host::{
    BlockNode, FlowPosition, LeafGeometry, LinkMark, RenderContext, Table, collect_leaf_geometry,
};
use gpui_kit::component::{ActiveTheme as _, input::EditorState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use unicode_segmentation::UnicodeSegmentation as _;

use super::caret::Caret;
use super::document::{self, EditResult, Kind, VisualDocument};
use super::perf::{Perf, ms};
use super::prefs::FocusMode;
use super::rows::Rows;

actions!(
    visual_editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveLineStart,
        MoveLineEnd,
        Backspace,
        Delete,
        Enter,
        Undo,
        Redo,
        InsertImage,
        Cancel
    ]
);

pub const CONTEXT: &str = "VisualEditor";

pub type PasteImages = Rc<dyn Fn(&ClipboardItem, &mut App) -> Option<Vec<(String, String)>>>;

pub const OPEN_PREFIX_BYTES: usize = 12 * 1024;

pub const BODY_SIZE: f32 = 17.;
pub const PAGE_TOP: f32 = 28.;
pub const PAGE_BOTTOM: f32 = 96.;
const FOCUS_DIM: f32 = 0.62;
const FOCUS_FADE: Duration = Duration::from_millis(160);

#[derive(Clone, Debug, PartialEq)]
pub struct FocusTarget {
    pub block: u64,
    pub leaf: usize,
    pub key: u64,
    pub range: Range<usize>,
}

impl FocusTarget {
    fn same_place(&self, other: &FocusTarget) -> bool {
        self.block == other.block
            && self.leaf == other.leaf
            && self.range.start == other.range.start
    }
}

fn complement(area: Bounds<Pixels>, holes: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let mut ys = vec![area.top(), area.bottom()];
    for hole in holes {
        ys.push(hole.top().clamp(area.top(), area.bottom()));
        ys.push(hole.bottom().clamp(area.top(), area.bottom()));
    }
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.dedup();
    let mut out = Vec::new();
    for band in ys.windows(2) {
        let (top, bottom) = (band[0], band[1]);
        if bottom <= top {
            continue;
        }
        let mut covered: Vec<(Pixels, Pixels)> = holes
            .iter()
            .filter(|h| h.top() <= top && h.bottom() >= bottom)
            .map(|h| (h.left().max(area.left()), h.right().min(area.right())))
            .collect();
        covered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut x = area.left();
        for (left, right) in covered {
            if left > x {
                out.push(Bounds::new(point(x, top), size(left - x, bottom - top)));
            }
            x = x.max(right);
        }
        if x < area.right() {
            out.push(Bounds::new(
                point(x, top),
                size(area.right() - x, bottom - top),
            ));
        }
    }
    out
}

pub const SETTLE_AFTER: Duration = Duration::from_millis(300);

const SCROLL_HISTORY: usize = 512;

pub fn init(cx: &mut App) {
    super::commands::init(cx);
    let (undo, redo, redo_alt) = if cfg!(target_os = "macos") {
        ("cmd-z", "cmd-shift-z", "cmd-y")
    } else {
        ("ctrl-z", "ctrl-shift-z", "ctrl-y")
    };
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, Some(CONTEXT)),
        KeyBinding::new("right", MoveRight, Some(CONTEXT)),
        KeyBinding::new("up", MoveUp, Some(CONTEXT)),
        KeyBinding::new("down", MoveDown, Some(CONTEXT)),
        KeyBinding::new("home", MoveLineStart, Some(CONTEXT)),
        KeyBinding::new("end", MoveLineEnd, Some(CONTEXT)),
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("enter", Enter, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-i"
            } else {
                "ctrl-shift-i"
            },
            InsertImage,
            Some(CONTEXT),
        ),
        KeyBinding::new(undo, Undo, Some(CONTEXT)),
        KeyBinding::new(redo, Redo, Some(CONTEXT)),
        KeyBinding::new(redo_alt, Redo, Some(CONTEXT)),
    ]);
}

#[derive(Default)]
struct Background {
    generation: u64,
    task: Option<Task<()>>,
    loading: bool,
    settle: Option<Task<()>>,
}

pub struct VisualView {
    pub editor: Entity<EditorState>,
    pub doc: VisualDocument,
    pub list: ListState,
    pub caret: usize,
    pub anchor: Option<usize>,
    dragging: bool,
    goal_x: Option<Pixels>,
    pub marked: Option<Range<usize>>,
    pub geometry: Rc<RefCell<HashMap<u64, LeafGeometry>>>,
    pending_vertical: Option<bool>,
    pending_reveal: Option<(usize, u8)>,
    pending_place: Option<(Pixels, u8)>,
    guide_drag: Option<f32>,
    pub(super) selected_image: Option<u64>,
    pub(super) image_resize: Option<super::image_ui::ImageResize>,
    pub(super) image_bounds: Rc<RefCell<HashMap<u64, Bounds<Pixels>>>>,
    pub(super) image_dialog: Option<super::image_ui::ImageDialog>,
    pub paste_images: Option<PasteImages>,
    focus_target: Option<FocusTarget>,
    focus_fade: Option<(FocusTarget, std::time::Instant)>,
    padded: (usize, Pixels, Pixels),
    pub focus: FocusHandle,
    pub perf: Rc<RefCell<Perf>>,
    pub caret_blink: Caret,
    render_cx: RenderContext,
    pub(super) base: Option<std::path::PathBuf>,
    render_defs: String,
    synced_text: Rope,
    synced_revision: u64,
    hint: Option<(Range<usize>, usize)>,
    background: Background,
    scroll_history: Vec<(usize, ListOffset)>,
    pub last_edit: Option<EditResult>,
    pub rows: Rows,
    table_widths: HashMap<u64, (usize, Arc<Vec<f32>>)>,
    table_dirty: HashMap<u64, Vec<usize>>,
    _observe: Subscription,
    _prefs: Subscription,
}

impl VisualView {
    pub fn new(editor: Entity<EditorState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = window;
        let (text, revision) = {
            let state = editor.read(cx);
            (state.text().clone(), state.document_revision())
        };
        let source = text.to_string();
        let cut = document::prefix_cut(&source, OPEN_PREFIX_BYTES);
        let doc = VisualDocument::from_blocks(document::parse_blocks(&source[..cut], 0), &text);
        let rows = Rows::build(&doc);
        let list = ListState::new(rows.len(), ListAlignment::Top, px(600.));
        let render_cx = RenderContext::from_app(cx).with_body_font_size(px(BODY_SIZE));
        let caret = doc
            .blocks
            .iter()
            .find(|b| !b.leaves.is_empty())
            .map_or(0, |b| b.start + b.leaves[0].leaf_to_src(0));
        let observe = cx.observe(&editor, |this, _, cx| this.sync(cx));
        let prefs = cx.observe_global::<super::prefs::WritingPrefs>(|this, cx| {
            this.caret_blink.reset(cx);
            cx.notify();
        });
        let mut view = Self {
            editor,
            doc,
            list,
            caret,
            anchor: None,
            dragging: false,
            goal_x: None,
            marked: None,
            geometry: Rc::default(),
            pending_vertical: None,
            pending_reveal: None,
            pending_place: None,
            guide_drag: None,
            selected_image: None,
            image_resize: None,
            image_bounds: Rc::default(),
            image_dialog: None,
            paste_images: None,
            focus_target: None,
            focus_fade: None,
            padded: (0, px(0.), px(0.)),
            focus: cx.focus_handle(),
            perf: Rc::new(RefCell::new(Perf::from_env())),
            caret_blink: Caret::new(cx),
            render_cx,
            base: None,
            render_defs: String::new(),
            synced_text: text,
            synced_revision: revision,
            hint: None,
            background: Background::default(),
            scroll_history: Vec::new(),
            last_edit: None,
            rows,
            table_widths: HashMap::new(),
            table_dirty: HashMap::new(),
            _observe: observe,
            _prefs: prefs,
        };
        if cut < source.len() {
            view.background.loading = true;
            view.schedule_full_parse(cx);
        }
        view
    }

    pub fn loading(&self) -> bool {
        self.background.loading
    }

    pub fn parse_pending(&self) -> bool {
        self.background.task.is_some()
    }

    pub fn sync(&mut self, cx: &mut Context<Self>) {
        let (revision, rope, cursor) = {
            let state = self.editor.read(cx);
            (
                state.document_revision(),
                state.text().clone(),
                state.cursor(),
            )
        };
        if revision == self.synced_revision {
            return;
        }
        let expected_len = |(old, len): &(Range<usize>, usize)| {
            self.synced_text.len() - old.len() + len == rope.len()
        };
        let change = match self.hint.take().filter(expected_len) {
            Some(hint) => Some(hint),
            None => document::diff(&self.synced_text, &rope),
        };
        self.synced_text = rope;
        self.synced_revision = revision;
        let Some((old, new_len)) = change else {
            return;
        };
        let result = self.doc.edit(&self.synced_text, old.clone(), new_len);
        self.follow_splice(
            result.splice.old.clone(),
            result.splice.new_len,
            result.inner.clone(),
            result.path == document::Path::TableRow,
        );
        self.perf
            .borrow_mut()
            .reparsed(result.parsed_bytes, result.path);
        if result.needs_full {
            self.schedule_full_parse(cx);
        } else if result.parsed_bytes > 0 {
            self.schedule_settle(cx);
        }
        self.last_edit = Some(result);
        let caret = if self.caret <= old.start {
            self.caret
        } else if self.caret >= old.end {
            self.caret - old.len() + new_len
        } else {
            old.start + new_len
        };
        let _ = cursor;
        self.caret = self.doc.snap(caret.min(self.synced_text.len()));
        if let Some(marked) = &self.marked
            && marked.end > self.synced_text.len()
        {
            self.marked = None;
        }
        cx.notify();
    }

    fn follow_splice(
        &mut self,
        old: Range<usize>,
        new_len: usize,
        inner: Option<Range<usize>>,
        table_row: bool,
    ) {
        for block in &self.doc.blocks[old.start..old.start + new_len] {
            match (&inner, table_row) {
                (Some(rows), true) => self
                    .table_dirty
                    .entry(block.id)
                    .or_default()
                    .extend(rows.clone()),
                _ => {
                    self.table_widths.remove(&block.id);
                    self.table_dirty.remove(&block.id);
                }
            }
        }
        let (rows, count) = self.rows.splice(&self.doc, old, new_len, inner);
        self.splice_list(rows, count);
    }

    fn splice_list(&mut self, old: Range<usize>, new_len: usize) {
        let top = self.list.logical_scroll_top();
        self.list.splice(old.clone(), new_len);
        if old.contains(&top.item_ix) && new_len > 0 {
            let item_ix = top.item_ix.min(old.start + new_len - 1);
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: top.offset_in_item,
            });
        }
    }

    fn schedule_full_parse(&mut self, cx: &mut Context<Self>) {
        self.background.generation += 1;
        let generation = self.background.generation;
        let snapshot = self.synced_text.clone();
        let revision = self.synced_revision;
        let worker = snapshot.clone();
        let parse = cx
            .background_executor()
            .spawn(async move { document::parse_blocks(&worker.to_string(), 0) });
        self.background.task = Some(cx.spawn(async move |this, cx| {
            let blocks = parse.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_full_parse(generation, snapshot, revision, blocks, cx)
            });
        }));
    }

    fn schedule_settle(&mut self, cx: &mut Context<Self>) {
        self.background.settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                this.background.settle = None;
                if this.background.task.is_none() {
                    this.schedule_full_parse(cx);
                }
            });
        }));
    }

    fn finish_full_parse(
        &mut self,
        generation: u64,
        snapshot: Rope,
        revision: u64,
        blocks: Vec<document::Block>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.background.generation {
            return;
        }
        self.background.task = None;
        self.sync(cx);
        let mut fresh = VisualDocument::from_blocks(blocks, &snapshot);
        if revision != self.synced_revision
            && let Some((old, new_len)) = document::diff(&snapshot, &self.synced_text)
        {
            let result = fresh.edit(&self.synced_text, old, new_len);
            if result.needs_full {
                self.schedule_full_parse(cx);
            }
        }
        let splice = self.doc.reconcile(fresh.into_blocks(), &self.synced_text);
        self.follow_splice(splice.old, splice.new_len, None, false);
        self.background.loading = false;
        self.caret = self.doc.snap(self.caret);
        cx.notify();
    }

    pub fn edit(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remember_scroll();
        self.hint = Some((range.clone(), text.len()));
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(range.clone(), cx);
            state.replace(text.to_string(), window, cx);
        });
        self.sync(cx);
        self.caret = range.start + text.len();
        self.anchor = None;
        self.goal_x = None;
        self.after_caret_change(cx);
        self.perf.borrow_mut().handler_done();
    }

    pub fn selection(&self) -> Range<usize> {
        match self.anchor {
            Some(anchor) => anchor.min(self.caret)..anchor.max(self.caret),
            None => self.caret..self.caret,
        }
    }

    pub fn edit_and_select(
        &mut self,
        range: Range<usize>,
        text: &str,
        select: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(range, text, window, cx);
        self.anchor = (!select.is_empty()).then_some(select.start);
        self.caret = select.end;
        self.after_caret_change(cx);
    }

    fn begin_move(&mut self, extend: bool, forward: bool) -> Option<usize> {
        self.selected_image = None;
        if extend {
            self.anchor.get_or_insert(self.caret);
            return None;
        }
        let selection = self.selection();
        self.anchor = None;
        let edge = if forward {
            selection.end
        } else {
            selection.start
        };
        (!selection.is_empty()).then_some(edge)
    }

    fn remember_scroll(&mut self) {
        if self.scroll_history.len() >= SCROLL_HISTORY {
            self.scroll_history.drain(..SCROLL_HISTORY / 2);
        }
        self.scroll_history
            .push((self.caret, self.list.logical_scroll_top()));
    }

    fn after_caret_change(&mut self, cx: &mut Context<Self>) {
        self.caret_blink.reset(cx);
        if let Some(y) = self.guide_y(cx) {
            self.pending_reveal = None;
            if !self.place_caret_step(y) {
                self.pending_place = Some((y, 4));
            }
        } else if let Some(row) = self.caret_row()
            && !self.reveal_caret_step(row)
        {
            self.pending_reveal = Some((row, 4));
        }
        cx.notify();
    }

    pub fn focus_target(&self, mode: FocusMode) -> Option<FocusTarget> {
        let (bix, lix, index) = self.doc.locate(self.caret)?;
        let block = &self.doc.blocks[bix];
        let leaf = &block.leaves[lix];
        let range = match mode {
            FocusMode::Off => return None,
            FocusMode::Paragraph => 0..leaf.len(),
            FocusMode::Sentence => super::focus::sentence_at(&leaf.text, index),
        };
        Some(FocusTarget {
            block: block.id,
            leaf: lix,
            key: leaf.key,
            range,
        })
    }

    pub fn focus_fade_active(&self) -> Option<&FocusTarget> {
        self.focus_fade.as_ref().map(|(old, _)| old)
    }

    fn update_focus(&mut self, cx: &App) {
        let mode = super::prefs::WritingPrefs::get(cx).focus;
        let target = self.focus_target(mode);
        let moved = match (&self.focus_target, &target) {
            (Some(old), Some(new)) => !old.same_place(new),
            _ => false,
        };
        if moved && !cx.reduce_motion() {
            self.focus_fade = self
                .focus_target
                .clone()
                .map(|old| (old, std::time::Instant::now()));
        }
        if self
            .focus_fade
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() >= FOCUS_FADE)
            || target.is_none()
            || cx.reduce_motion()
        {
            self.focus_fade = None;
        }
        self.focus_target = target;
    }

    fn focus_lines(
        &self,
        target: &FocusTarget,
        geometry: &HashMap<u64, LeafGeometry>,
    ) -> Vec<Bounds<Pixels>> {
        let Some(g) = geometry.get(&target.key) else {
            return Vec::new();
        };
        let Some((bix, lix)) = self.doc.leaf_by_key(target.key) else {
            return Vec::new();
        };
        let text = &self.doc.blocks[bix].leaves[lix].text;
        let end = target.range.end.min(text.len());
        let start = target.range.start.min(end);
        let line_height = g.line_height();
        let mut lines: Vec<(Pixels, Pixels, Pixels)> = Vec::new();
        let boundaries = text[start..end]
            .grapheme_indices(true)
            .map(|(i, _)| start + i)
            .chain(std::iter::once(end));
        for i in boundaries {
            let p = g.position_for_index(i);
            match lines.iter_mut().find(|(y, _, _)| *y == p.y) {
                Some(line) => {
                    line.1 = line.1.min(p.x);
                    line.2 = line.2.max(p.x);
                }
                None => lines.push((p.y, p.x, p.x)),
            }
        }
        lines
            .into_iter()
            .map(|(y, left, right)| {
                Bounds::new(
                    point(left - px(2.), y),
                    size(right - left + px(4.), line_height),
                )
            })
            .collect()
    }

    fn paint_focus(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) {
        let Some(target) = &self.focus_target else {
            return;
        };
        let geometry = self.geometry.borrow();
        let progress = self.focus_fade.as_ref().map_or(1., |(_, start)| {
            (start.elapsed().as_secs_f32() / FOCUS_FADE.as_secs_f32()).min(1.)
        });
        let mut clear: Vec<(Bounds<Pixels>, f32)> = self
            .focus_lines(target, &geometry)
            .into_iter()
            .map(|line| (line, FOCUS_DIM * (1. - progress)))
            .collect();
        if let Some((old, _)) = &self.focus_fade {
            clear.extend(
                self.focus_lines(old, &geometry)
                    .into_iter()
                    .map(|line| (line, FOCUS_DIM * progress)),
            );
        }
        let veil = cx.theme().background;
        let first = self.list.logical_scroll_top().item_ix;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for ix in first..self.rows.len() {
                let Some(row) = self.list.bounds_for_item(ix) else {
                    break;
                };
                if row.top() > bounds.bottom() {
                    break;
                }
                let row = Bounds::new(
                    point(row.left() - px(8.), row.top()),
                    size(row.size.width + px(16.), row.size.height),
                );
                let inside: Vec<Bounds<Pixels>> = clear
                    .iter()
                    .map(|(line, _)| *line)
                    .filter(|line| line.bottom() > row.top() && line.top() < row.bottom())
                    .collect();
                for gap in complement(row, &inside) {
                    window.paint_quad(fill(gap, veil.opacity(FOCUS_DIM)));
                }
            }
            for (line, alpha) in &clear {
                if *alpha > 0.01 {
                    window.paint_quad(fill(*line, veil.opacity(*alpha)));
                }
            }
        });
        if self.focus_fade.is_some() {
            window.request_animation_frame();
        }
    }

    pub fn guide_fraction(&self, cx: &App) -> Option<f32> {
        let prefs = super::prefs::WritingPrefs::get(cx);
        prefs
            .typewriter
            .then(|| self.guide_drag.unwrap_or(prefs.typewriter_guide))
    }

    pub fn guide_y(&self, cx: &App) -> Option<Pixels> {
        let fraction = self.guide_fraction(cx)?;
        let height = self.list.viewport_bounds().size.height;
        (height > px(0.)).then(|| height * fraction)
    }

    pub fn guide_line_y(&self, cx: &App) -> Option<Pixels> {
        let line = self
            .caret_position()
            .map_or(px(BODY_SIZE * 1.6), |(_, line_height, _)| line_height);
        Some(self.guide_y(cx)? + line)
    }

    fn edge_padding(&self, cx: &App) -> (Pixels, Pixels) {
        let height = self.list.viewport_bounds().size.height;
        match self.guide_fraction(cx) {
            Some(fraction) if height > px(0.) => (
                (height * fraction - px(BODY_SIZE * 2.)).max(px(0.)),
                (height * (1. - fraction)).max(px(PAGE_BOTTOM)),
            ),
            _ => (px(0.), px(PAGE_BOTTOM)),
        }
    }

    fn refresh_edge_padding(&mut self, cx: &App) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        let (head, tail) = self.edge_padding(cx);
        let wanted = (count - 1, head, tail);
        if wanted == self.padded {
            return;
        }
        let (old_last, old_head, _) = self.padded;
        self.padded = wanted;
        let mut stale = vec![count - 1];
        if old_head != head {
            stale.push(0);
        }
        if old_last < count && old_last != count - 1 {
            stale.push(old_last);
        }
        stale.sort_unstable();
        stale.dedup();
        for ix in stale {
            self.splice_list(ix..ix + 1, 1);
        }
    }

    fn drag_guide(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let viewport = self.list.viewport_bounds();
        if viewport.size.height <= px(0.) {
            return;
        }
        let line = self
            .caret_position()
            .map_or(px(BODY_SIZE * 1.6), |(_, line_height, _)| line_height);
        let mut fraction = ((y - line - viewport.top()) / viewport.size.height).clamp(0.15, 0.85);
        if (fraction - 0.5).abs() < 0.03 {
            fraction = 0.5;
        }
        self.guide_drag = Some(fraction);
        if let Some(y) = self.guide_y(cx) {
            self.place_caret_step(y);
            self.pending_place = Some((y, 2));
        }
        cx.notify();
    }

    fn release_guide(&mut self, cx: &mut Context<Self>) {
        if let Some(fraction) = self.guide_drag.take() {
            let mut prefs = super::prefs::WritingPrefs::get(cx);
            prefs.typewriter_guide = fraction;
            cx.set_global(prefs);
        }
    }

    fn guide(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let y = self.guide_line_y(cx)?;
        let theme = cx.theme();
        let color = theme.muted_foreground;
        let active = self.guide_drag.is_some();
        let dashes = (0..56).map(move |_| div().w(px(6.)).h(px(1.)).flex_shrink_0().bg(color));
        Some(
            div()
                .id("typewriter-guide")
                .absolute()
                .top(px(PAGE_TOP) + y - px(6.))
                .left_0()
                .right_0()
                .h(px(13.))
                .flex()
                .justify_center()
                .cursor_row_resize()
                .opacity(if active { 0.7 } else { 0.18 })
                .hover(|guide| guide.opacity(0.55))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.guide_drag = this.guide_fraction(cx);
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(680.))
                        .max_w_full()
                        .px_4()
                        .h_full()
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .child(div().flex().gap(px(6.)).children(dashes)),
                )
                .into_any_element(),
        )
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        if self.delete_selected_image(window, cx) {
            return;
        }
        let selection = self.selection();
        if !selection.is_empty() {
            self.edit(selection, "", window, cx);
            return;
        }
        let Some((bix, lix, index)) = self.doc.locate(self.caret) else {
            return;
        };
        let leaf = &self.doc.blocks[bix].leaves[lix];
        if index > 0 {
            let prev = leaf.snap_index(prev_boundary(&leaf.text, index), false);
            let start = self.doc.src_of(bix, lix, prev);
            let end = self.caret.max(start);
            let end = if end == start {
                self.doc.src_of(bix, lix, index)
            } else {
                end
            };
            self.edit(start..end, "", window, cx);
        } else if let Some((pb, pl)) = self.doc.step_leaf(bix, lix, false) {
            let prev_len = self.doc.blocks[pb].leaves[pl].len();
            let start = self.doc.src_of(pb, pl, prev_len);
            if start < self.caret {
                self.edit(start..self.caret, "", window, cx);
            }
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        if self.delete_selected_image(window, cx) {
            return;
        }
        let selection = self.selection();
        if !selection.is_empty() {
            self.edit(selection, "", window, cx);
            return;
        }
        let Some((bix, lix, index)) = self.doc.locate(self.caret) else {
            return;
        };
        let leaf = &self.doc.blocks[bix].leaves[lix];
        if index < leaf.len() {
            let next = leaf.snap_index(next_boundary(&leaf.text, index), true);
            let end = self.doc.src_of(bix, lix, next);
            if end > self.caret {
                self.edit(self.caret..end, "", window, cx);
            }
        }
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        self.enter_command(window, cx);
        self.perf.borrow_mut().handler_done();
    }

    pub fn replay(&mut self, undo: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        self.marked = None;
        self.editor.update(cx, |state, cx| {
            if undo {
                state.undo(&gpui_kit::component::input::Undo, window, cx);
            } else {
                state.redo(&gpui_kit::component::input::Redo, window, cx);
            }
        });
        self.sync(cx);
        let selection = self.editor.read(cx).selected_range();
        self.caret = self.doc.snap(selection.start.min(self.synced_text.len()));
        self.goal_x = None;
        let restored = undo
            && self
                .scroll_history
                .iter()
                .rev()
                .find(|(caret, _)| *caret == selection.start)
                .map(|(_, offset)| *offset)
                .map(|offset| self.list.scroll_to(offset))
                .is_some();
        if restored {
            self.pending_reveal = None;
        }
        self.caret_blink.reset(cx);
        if !restored {
            self.reveal_caret_if_hidden();
        }
        self.perf.borrow_mut().handler_done();
        cx.notify();
    }

    fn caret_row(&self) -> Option<usize> {
        let (bix, lix, _) = self.doc.locate(self.caret)?;
        Some(self.rows.row_of(&self.doc, bix, lix))
    }

    fn reveal_caret_step(&mut self, row: usize) -> bool {
        let viewport = self.list.viewport_bounds();
        let Some((top, line_height, _)) = self.caret_position() else {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
            return false;
        };
        let margin = line_height.min(viewport.size.height / 4.);
        if top.y >= viewport.top() + margin && top.y + line_height <= viewport.bottom() - margin {
            return true;
        }
        let Some(item) = self.list.bounds_for_item(row) else {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
            return false;
        };
        let within = top.y - item.top();
        let goal = if top.y < viewport.top() + margin {
            margin
        } else {
            viewport.size.height - margin - line_height
        };
        if within >= goal {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: within - goal,
            });
        } else {
            self.list.scroll_by(top.y - (viewport.top() + goal));
        }
        false
    }

    fn reveal_caret_if_hidden(&mut self) {
        if let Some(row) = self.caret_row()
            && !self.reveal_caret_step(row)
        {
            self.pending_reveal = Some((row, 4));
        }
    }

    pub(super) fn move_horizontal(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        if let Some(edge) = self.begin_move(extend, forward) {
            self.caret = edge;
            self.goal_x = None;
            self.after_caret_change(cx);
            return;
        }
        let Some((bix, lix, index)) = self.doc.locate(self.caret) else {
            return;
        };
        let leaf = &self.doc.blocks[bix].leaves[lix];
        let target = if forward && index < leaf.len() {
            let next = next_boundary(&leaf.text, index);
            Some((bix, lix, leaf.snap_index(next, true)))
        } else if !forward && index > 0 {
            let prev = prev_boundary(&leaf.text, index);
            Some((bix, lix, leaf.snap_index(prev, false)))
        } else {
            self.doc.step_leaf(bix, lix, forward).map(|(b, l)| {
                let len = self.doc.blocks[b].leaves[l].len();
                (b, l, if forward { 0 } else { len })
            })
        };
        if let Some((b, l, i)) = target {
            self.caret = self.doc.src_of(b, l, i);
        }
        self.goal_x = None;
        self.after_caret_change(cx);
    }

    pub fn set_base(&mut self, base: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        self.base = base;
        self.refresh_render_cx(cx);
        cx.notify();
    }

    fn refresh_render_cx(&mut self, cx: &App) {
        let images = self.base.clone();
        let base = self.base.clone();
        let mut render_cx = RenderContext::from_app(cx)
            .with_body_font_size(px(BODY_SIZE))
            .with_image_source(move |uri| {
                let source = uri.to_string();
                match &images {
                    Some(base) if !source.contains(':') => ImageSource::from(base.join(source)),
                    _ => ImageSource::from(uri.clone()),
                }
            })
            .on_link_click(move |url, event, _, cx| {
                if !event.modifiers().secondary(cx) {
                    return;
                }
                if url.starts_with("https://")
                    || url.starts_with("http://")
                    || url.starts_with("mailto:")
                {
                    cx.open_url(url);
                } else if !url.contains(':')
                    && !url.starts_with('#')
                    && let Some(base) = &base
                {
                    cx.reveal_path(&base.join(url.as_ref()));
                }
            });
        for (identifier, url, title) in self.doc.link_references().into_iter().rev() {
            render_cx = render_cx.with_link_reference(
                identifier.clone(),
                LinkMark {
                    url: url.into(),
                    identifier: Some(identifier.into()),
                    title: title.map(Into::into),
                },
            );
        }
        self.render_defs = self.doc.definitions().to_string();
        self.render_cx = render_cx;
    }

    pub fn place_caret(&mut self, offset: usize, y: Option<Pixels>, cx: &mut Context<Self>) {
        self.anchor = None;
        self.marked = None;
        self.goal_x = None;
        self.caret = self.doc.snap(offset.min(self.synced_text.len()));
        self.pending_reveal = None;
        match (y, self.caret_row()) {
            (Some(y), Some(row)) => {
                self.list.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(0.),
                });
                self.pending_place = Some((y, 4));
            }
            _ => self.reveal_caret_if_hidden(),
        }
        self.caret_blink.reset(cx);
        cx.notify();
    }

    pub fn caret_viewport_y(&self) -> Option<Pixels> {
        let viewport = self.list.viewport_bounds();
        let (top, line_height, _) = self.caret_position()?;
        (top.y >= viewport.top() && top.y + line_height <= viewport.bottom())
            .then(|| top.y - viewport.top())
    }

    fn place_caret_step(&mut self, y: Pixels) -> bool {
        let viewport = self.list.viewport_bounds();
        let Some((top, _, _)) = self.caret_position() else {
            if let Some(row) = self.caret_row() {
                self.list.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(0.),
                });
            }
            return false;
        };
        let delta = top.y - (viewport.top() + y);
        if delta.abs() < px(1.) {
            return true;
        }
        self.list.scroll_by(delta);
        false
    }

    pub fn caret_position(&self) -> Option<(Point<Pixels>, Pixels, Pixels)> {
        let (bix, lix, index) = self.doc.locate(self.caret)?;
        let key = self.doc.blocks[bix].leaves[lix].key;
        let geometry = self.geometry.borrow();
        let leaf = geometry.get(&key)?;
        Some((
            leaf.position_for_index(index),
            leaf.line_height(),
            leaf.caret_height(),
        ))
    }

    fn try_vertical(&mut self, down: bool) -> bool {
        let Some((top, line_height, _)) = self.caret_position() else {
            return false;
        };
        let goal = *self.goal_x.get_or_insert(top.x);
        let geometry = self.geometry.borrow();
        let mut y = if down {
            top.y + line_height + px(1.)
        } else {
            top.y - px(1.)
        };
        for _ in 0..600 {
            let hit = geometry
                .iter()
                .filter(|(_, g)| {
                    let b = g.bounds();
                    b.top() <= y && y < b.bottom()
                })
                .min_by(|a, b| {
                    let d = |g: &LeafGeometry| {
                        let b = g.bounds();
                        if goal < b.left() {
                            b.left() - goal
                        } else if goal > b.right() {
                            goal - b.right()
                        } else {
                            px(0.)
                        }
                    };
                    d(a.1)
                        .partial_cmp(&d(b.1))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            if let Some((&key, g)) = hit {
                let index = g.index_for_position(point(goal, y));
                let at = g.position_for_index(index);
                let moved = if down {
                    at.y > top.y + px(0.5)
                } else {
                    at.y < top.y - px(0.5)
                };
                if moved && let Some((bix, lix)) = self.doc.leaf_by_key(key) {
                    self.caret = self.doc.src_of(bix, lix, index);
                    return true;
                }
            }
            y += if down { px(2.) } else { px(-2.) };
        }
        false
    }

    pub(super) fn move_vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        self.begin_move(extend, down);
        if self.try_vertical(down) {
            self.pending_vertical = None;
            self.caret_blink.reset(cx);
            cx.notify();
            return;
        }
        let step = self.caret_position().map_or(px(40.), |(_, h, _)| h * 2.);
        self.list.scroll_by(if down { step } else { -step });
        self.pending_vertical = Some(down);
        cx.notify();
    }

    pub(super) fn line_edge(&mut self, end: bool, extend: bool, cx: &mut Context<Self>) {
        self.perf.borrow_mut().input();
        self.begin_move(extend, end);
        let Some((top, line_height, _)) = self.caret_position() else {
            return;
        };
        let Some((bix, lix, _)) = self.doc.locate(self.caret) else {
            return;
        };
        let key = self.doc.blocks[bix].leaves[lix].key;
        let index = {
            let geometry = self.geometry.borrow();
            let Some(g) = geometry.get(&key) else { return };
            let x = if end { px(1e6) } else { px(-1e6) };
            g.index_for_position(point(x, top.y + line_height / 2.))
        };
        self.caret = self.doc.src_of(bix, lix, index);
        self.goal_x = None;
        self.after_caret_change(cx);
    }

    pub fn source_for_point(&self, at: Point<Pixels>) -> Option<usize> {
        let geometry = self.geometry.borrow();
        let dist = |g: &LeafGeometry| {
            let b = g.bounds();
            let dy = if at.y < b.top() {
                b.top() - at.y
            } else if at.y > b.bottom() {
                at.y - b.bottom()
            } else {
                px(0.)
            };
            let dx = if at.x < b.left() {
                b.left() - at.x
            } else if at.x > b.right() {
                at.x - b.right()
            } else {
                px(0.)
            };
            (dy, dx)
        };
        let (&key, g) = geometry.iter().min_by(|a, b| {
            dist(a.1)
                .partial_cmp(&dist(b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let (bix, lix) = self.doc.leaf_by_key(key)?;
        let text = &self.doc.blocks[bix].leaves[lix].text;
        let index = g.index_for_position(at);
        if index > text.len() || !text.is_char_boundary(index) {
            return Some(self.doc.src_of(bix, lix, index));
        }
        let line_y = g.position_for_index(index).y;
        let index = [
            prev_boundary(text, index),
            index,
            next_boundary(text, index),
        ]
        .into_iter()
        .map(|i| (i, g.position_for_index(i)))
        .filter(|(_, p)| p.y == line_y)
        .min_by(|a, b| {
            (a.1.x - at.x)
                .abs()
                .partial_cmp(&(b.1.x - at.x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(index, |(i, _)| i);
        Some(self.doc.src_of(bix, lix, index))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.perf.borrow_mut().input();
        window.focus(&self.focus, cx);
        self.selected_image = None;
        if let Some(src) = self.source_for_point(event.position) {
            if event.modifiers.shift {
                self.anchor.get_or_insert(self.caret);
            } else {
                self.anchor = None;
            }
            self.caret = src;
            self.marked = None;
            self.goal_x = None;
            self.dragging = true;
            match event.click_count {
                2 => self.select_word(cx),
                n if n >= 3 => self.select_paragraph(cx),
                _ => {}
            }
            self.caret_blink.reset(cx);
            cx.notify();
        }
        self.perf.borrow_mut().handler_done();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.image_resize.is_some() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.image_resize_move(event.position.x, cx);
            }
            return;
        }
        if self.guide_drag.is_some() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.drag_guide(event.position.y, cx);
            } else {
                self.release_guide(cx);
            }
            return;
        }
        if !self.dragging || event.pressed_button != Some(MouseButton::Left) {
            self.dragging = false;
            return;
        }
        let viewport = self.list.viewport_bounds();
        let edge = px(24.);
        if event.position.y < viewport.top() + edge {
            self.list
                .scroll_by((event.position.y - viewport.top() - edge).min(px(-4.)));
        } else if event.position.y > viewport.bottom() - edge {
            self.list
                .scroll_by((event.position.y - viewport.bottom() + edge).max(px(4.)));
        }
        if let Some(src) = self.source_for_point(event.position) {
            self.anchor.get_or_insert(self.caret);
            self.caret = src;
            if self.anchor == Some(self.caret) {
                self.anchor = None;
            }
            self.caret_blink.reset(cx);
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.dragging = false;
        self.image_resize_end(window, cx);
        self.release_guide(cx);
    }

    fn table_widths(
        &mut self,
        bix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Arc<Vec<f32>> {
        let block = &self.doc.blocks[bix];
        let Kind::Table { rows, aligns } = &block.kind else {
            return Arc::default();
        };
        let id = block.id;
        let table = |rows: Vec<Vec<_>>| Table::new(rows, aligns.clone());
        let measured = match self.table_widths.get(&id) {
            Some((count, widths)) if *count == rows.len() => {
                let dirty = self.table_dirty.remove(&id).unwrap_or_default();
                if dirty.is_empty() {
                    return widths.clone();
                }
                let mut sample = vec![rows[0].cells.clone()];
                sample.extend(
                    dirty
                        .iter()
                        .filter_map(|&r| rows.get(r))
                        .map(|r| r.cells.clone()),
                );
                let fresh = table(sample).measure_column_widths(&self.render_cx, window, cx);
                let mut widths = widths.to_vec();
                widths.resize(fresh.len().max(widths.len()), 0.);
                for (w, f) in widths.iter_mut().zip(fresh) {
                    *w = w.max(f);
                }
                widths
            }
            _ => {
                self.table_dirty.remove(&id);
                table(rows.iter().map(|r| r.cells.clone()).collect()).measure_column_widths(
                    &self.render_cx,
                    window,
                    cx,
                )
            }
        };
        let count = match &self.doc.blocks[bix].kind {
            Kind::Table { rows, .. } => rows.len(),
            _ => 0,
        };
        let measured = Arc::new(measured);
        self.table_widths.insert(id, (count, measured.clone()));
        measured
    }

    fn render_row(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(ix).cloned() else {
            return div().into_any_element();
        };
        let bix = row.block;
        if bix >= self.doc.blocks.len() {
            return div().into_any_element();
        }
        self.perf.borrow_mut().block_laid_out();
        let from = bix.saturating_sub(3);
        let refs: Vec<&BlockNode> = self.doc.blocks[from..=bix]
            .iter()
            .map(|b| &b.node)
            .collect();
        let flow =
            FlowPosition::at(&refs, bix - from, bix + 1 == self.doc.blocks.len()).first(bix == 0);
        let (last, head, tail) = self.padded;
        let pad_top = if ix == 0 { head } else { px(0.) };
        let pad_bottom = if ix == last { tail } else { px(0.) };
        if let Kind::Image(image) = &self.doc.blocks[bix].kind {
            let (id, image) = (self.doc.blocks[bix].id, image.clone());
            return self.render_image_block(id, &image, pad_top, pad_bottom, cx);
        }
        let Some(part) = row.part else {
            let block = &self.doc.blocks[bix];
            return div()
                .w_full()
                .pt(pad_top)
                .pb(pad_bottom)
                .child(block.node.render_with_id(
                    ElementId::Integer(block.id),
                    flow,
                    &self.render_cx,
                    window,
                    cx,
                ))
                .into_any_element();
        };
        let total = match &self.doc.blocks[bix].kind {
            Kind::List { items } => items.len(),
            Kind::Table { rows, .. } => rows.len(),
            _ => 0,
        };
        let mut flow = flow;
        if part.start > 0 {
            flow = flow.first(true);
        }
        if part.end < total {
            flow = flow.last(true);
        }
        let node = match (&self.doc.blocks[bix].node, &self.doc.blocks[bix].kind) {
            (
                BlockNode::List {
                    children,
                    ordered,
                    start,
                    ..
                },
                _,
            ) => BlockNode::List {
                children: children[part.clone()].to_vec(),
                ordered: *ordered,
                start: if *ordered {
                    Some(start.unwrap_or(1) + part.start as u32)
                } else {
                    *start
                },
                span: None,
            },
            (_, Kind::Table { .. }) => {
                let widths = self.table_widths(bix, window, cx);
                let Kind::Table { rows, aligns } = &self.doc.blocks[bix].kind else {
                    return div().into_any_element();
                };
                BlockNode::Table(
                    Table::new(
                        rows[part.clone()].iter().map(|r| r.cells.clone()).collect(),
                        aligns.clone(),
                    )
                    .chunk(
                        part.start == 0,
                        part.start == 0,
                        part.end == total,
                        widths,
                    ),
                )
            }
            _ => self.doc.blocks[bix].node.clone(),
        };
        let id = self.doc.blocks[bix].id;
        let continues_list = part.start > 0 && matches!(node, BlockNode::List { .. });
        div()
            .w_full()
            .when(continues_list, |row| row.pt(px(BODY_SIZE * 0.25)))
            .when(ix == 0, |row| row.pt(pad_top))
            .pb(pad_bottom)
            .child(node.render_with_id(
                ElementId::Integer((id << 20) | part.start as u64),
                flow,
                &self.render_cx,
                window,
                cx,
            ))
            .into_any_element()
    }

    fn decorations(&self, cx: &App) -> HashMap<u64, Vec<(Range<usize>, Hsla)>> {
        let mut out: HashMap<u64, Vec<(Range<usize>, Hsla)>> = HashMap::new();
        let selection = self.selection();
        if !selection.is_empty() {
            let color = cx.theme().selection;
            let first = self.doc.blocks.partition_point(|b| b.end < selection.start);
            for block in &self.doc.blocks[first..] {
                if block.start > selection.end {
                    break;
                }
                for leaf in &block.leaves {
                    let src = leaf.src_range();
                    let (lo, hi) = (block.start + src.start, block.start + src.end);
                    if hi < selection.start || lo > selection.end {
                        continue;
                    }
                    let rel = |x: usize| x.saturating_sub(block.start);
                    let s = if selection.start <= lo {
                        0
                    } else {
                        leaf.src_to_leaf(rel(selection.start))
                    };
                    let e = if selection.end >= hi {
                        leaf.len()
                    } else {
                        leaf.src_to_leaf(rel(selection.end))
                    };
                    if e > s {
                        out.entry(leaf.key).or_default().push((s..e, color));
                    }
                }
            }
        }
        if let Some(marked) = &self.marked
            && let (Some((b, l, s)), Some((_, _, e))) =
                (self.doc.locate(marked.start), self.doc.locate(marked.end))
        {
            let key = self.doc.blocks[b].leaves[l].key;
            out.entry(key)
                .or_default()
                .push((s..e.max(s), cx.theme().selection));
        }
        out
    }

    fn perf_overlay(&self, cx: &App) -> Option<AnyElement> {
        let perf = self.perf.borrow();
        if !perf.overlay {
            return None;
        }
        let theme = cx.theme();
        let budget_120 = Duration::from_micros(8_333);
        let budget_60 = Duration::from_micros(16_667);
        let frames = &perf.frames[perf.frames.len().saturating_sub(120)..];
        let scale = 2.4;
        let bars = frames.iter().map(|f| {
            let height = (ms(*f) as f32 * scale).min(60.);
            let color = if *f > budget_60 {
                theme.danger
            } else if *f > budget_120 {
                theme.warning
            } else {
                theme.success
            };
            div().w(px(2.)).h(px(height)).bg(color)
        });
        let last = perf.samples.last();
        let line = |s: String| div().child(s);
        Some(
            div()
                .absolute()
                .top_2()
                .right_2()
                .p_2()
                .rounded_md()
                .bg(theme.popover)
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .font_family("monospace")
                .child(
                    div()
                        .h(px(60.))
                        .w(px(240.))
                        .flex()
                        .items_end()
                        .gap(px(0.))
                        .children(bars),
                )
                .child(line(format!(
                    "over 8.3 ms: {}  over 16.7 ms: {}",
                    perf.over_budget(budget_120),
                    perf.over_budget(budget_60)
                )))
                .child(line(match last {
                    Some(s) => format!(
                        "last input: work {:.2} ms (handler {:.2} + frame {:.2})",
                        ms(s.work()),
                        ms(s.handler),
                        ms(s.frame)
                    ),
                    None => "last input: none".into(),
                }))
                .child(line(match last {
                    Some(s) => format!(
                        "reparsed {} B ({:?}), {} blocks laid out",
                        s.parsed_bytes, s.path, s.blocks_laid_out
                    ),
                    None => String::new(),
                }))
                .into_any_element(),
        )
    }
}

pub fn next_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .grapheme_indices(true)
        .nth(1)
        .map_or(text.len(), |(i, _)| index + i)
}

pub fn prev_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}

impl Focusable for VisualView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for VisualView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.perf.borrow_mut().frame_begin();
        if self.doc.definitions() != self.render_defs {
            self.refresh_render_cx(cx);
        }
        self.refresh_edge_padding(cx);
        let guide = self.guide(cx);
        let theme = cx.theme();
        let list = list(self.list.clone(), cx.processor(Self::render_row))
            .w_full()
            .flex_1()
            .min_h_0();
        let loading = self.background.loading.then(|| {
            div()
                .absolute()
                .bottom_2()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Preparing the rest of the document…")
        });
        let page = div().size_full().flex().justify_center().child(
            div()
                .w(px(680.))
                .max_w_full()
                .h_full()
                .px_4()
                .pt(px(PAGE_TOP))
                .flex()
                .flex_col()
                .child(list),
        );
        let root = div()
            .id("visual-editor")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, _| {
                this.perf.borrow_mut().input();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_action(
                cx.listener(|this, _: &MoveLeft, _, cx| this.move_horizontal(false, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveRight, _, cx| this.move_horizontal(true, false, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_vertical(false, false, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_vertical(true, false, cx)))
            .on_action(
                cx.listener(|this, _: &MoveLineStart, _, cx| this.line_edge(false, false, cx)),
            )
            .on_action(cx.listener(|this, _: &MoveLineEnd, _, cx| this.line_edge(true, false, cx)))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(
                cx.listener(|this, _: &InsertImage, window, cx| {
                    this.insert_image_dialog(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Cancel, window, cx| {
                if this.image_dialog.is_some() {
                    this.close_image_dialog(window, cx);
                } else {
                    this.select_image(None, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Undo, window, cx| this.replay(true, window, cx)))
            .on_action(cx.listener(|this, _: &Redo, window, cx| this.replay(false, window, cx)));
        super::commands::listeners(root, cx)
            .child(Surface {
                view: cx.entity(),
                child: Some(page.into_any_element()),
                decorations: self.decorations(cx),
            })
            .children(guide)
            .children(self.render_image_dialog(cx))
            .children(loading)
            .children(self.perf_overlay(cx))
    }
}

struct Surface {
    view: Entity<VisualView>,
    child: Option<AnyElement>,
    decorations: HashMap<u64, Vec<(Range<usize>, Hsla)>>,
}

impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Surface {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut child = self.child.take().expect("surface child");
        let child_layout = child.request_layout(window, cx);
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Default::default()
        };
        (window.request_layout(style, [child_layout], cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        let decorations = std::mem::take(&mut self.decorations);
        let (_, geometry) = collect_leaf_geometry(decorations, || child.prepaint(window, cx));
        self.view.update(cx, |view, cx| {
            *view.geometry.borrow_mut() = geometry;
            if let Some((bix, tries)) = view.pending_reveal.take()
                && tries > 0
                && !view.reveal_caret_step(bix)
            {
                view.pending_reveal = Some((bix, tries - 1));
                cx.notify();
            }
            if let Some((y, tries)) = view.pending_place.take()
                && tries > 0
                && !view.place_caret_step(y)
            {
                view.pending_place = Some((y, tries - 1));
                cx.notify();
            }
            view.update_focus(cx);
            if let Some(down) = view.pending_vertical.take() {
                if !view.try_vertical(down) {
                    view.pending_vertical = Some(down);
                }
                cx.notify();
            }
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
        let view = self.view.read(cx);
        view.paint_focus(bounds, window, cx);
        let focused = view.focus.is_focused(window);
        if focused
            && view.selected_image.is_none()
            && view.caret_blink.visible()
            && let Some((top, line_height, caret_height)) = view.caret_position()
        {
            let prefs = super::prefs::WritingPrefs::get(cx);
            let y = top.y + (line_height - caret_height) / 2.;
            let width = px(prefs.caret_width);
            let x = top.x - px((prefs.caret_width / 2.).floor());
            let caret = Bounds::new(point(x, y), size(width, caret_height));
            let color = prefs.caret_hsla(cx.theme().caret, cx.theme().primary);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                window.paint_quad(fill(caret, color));
            });
        }
        let perf = view.perf.clone();
        let focus = view.focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        perf.borrow_mut().frame_end();
    }
}
