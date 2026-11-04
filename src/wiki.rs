//! Wiki completion, workspace graph jobs, and the small backlinks footer.
use super::*;
use crate::links::{self, Move};
use gpui_kit::component::{
    input::Rope,
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node},
};
use std::{ops::Range, time::Duration};

pub struct Completion {
    editor: EntityId,
    start: usize,
    end: usize,
    query: String,
    anchor: Point<Pixels>,
    selected: usize,
    choices: Vec<links::Note>,
    dismissed: bool,
    catalog: Arc<links::Catalog>,
    scroll: ScrollHandle,
}
#[derive(Clone)]
struct OpenNote {
    path: PathBuf,
    revision: u64,
    rope: Option<Rope>,
    saved: Arc<String>,
    dirty: bool,
}
impl OpenNote {
    fn text(&self) -> String {
        self.rope
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| self.saved.as_ref().clone())
    }
}

impl Enso {
    pub(crate) fn tree_change_busy(&self) -> bool {
        self.repairing_links || self.documents.iter().any(|d| d.saving)
    }
    pub(crate) fn after_tree_change(
        &mut self,
        run: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let root = self.root.clone();
        cx.spawn(async move |app, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let Ok(ready) = app.read_with(cx, |app, _| {
                    !app.tree_change_busy() && app.link_moves.is_empty()
                }) else {
                    return;
                };
                if ready {
                    break;
                }
            }
            let _ = app.update(cx, |app, cx| {
                if app.root == root {
                    run(app, cx);
                }
            });
        })
        .detach();
    }
    pub(crate) fn rename_entry_when_ready(
        &mut self,
        old: PathBuf,
        name: String,
        cx: &mut Context<Self>,
    ) {
        if self.tree_change_busy() {
            self.after_tree_change(
                move |app, cx| app.rename_entry_when_ready(old, name, cx),
                cx,
            );
            return;
        }
        match files::rename(&old, &name) {
            Ok(new) if new != old => self.entry_moved(&old, &new, cx),
            Ok(_) => {}
            Err(error) => self.message(error.to_string(), true, cx),
        }
    }
    pub(crate) fn queue_link_move(&mut self, old: &PathBuf, new: &PathBuf, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        self.link_moves.push(Move {
            old: old.clone(),
            new: new.clone(),
        });
        self.wiki_completion = None;
        self.refresh_links(cx);
    }
    /// One worker per window: metadata checks, link parsing and move repairs are
    /// serialized. Saves pause only while a move's link edits are being prepared.
    pub(crate) fn refresh_links(&mut self, cx: &mut Context<Self>) {
        if self.link_task.is_some() {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let index = self.link_index.clone();
        let handle = self.window_handle;
        self.link_task = Some(cx.spawn(async move |this, cx| {
            // Batch multi-file drops and let in-flight automatic saves finish.
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let Ok(busy) = this.read_with(cx, |app, _| app.documents.iter().any(|d| d.saving))
                else {
                    return;
                };
                if !busy {
                    break;
                }
            }
            let Ok(Some((moves, open))) = this.update(cx, |app, cx| {
                if app.root.as_ref() != Some(&root) {
                    app.link_task = None;
                    return None;
                }
                let moves = std::mem::take(&mut app.link_moves);
                app.repairing_links = !moves.is_empty();
                let open = app
                    .documents
                    .iter()
                    .filter(|d| d.markdown && d.path.as_ref().is_some_and(|p| p.starts_with(&root)))
                    .filter_map(|d| {
                        Some(OpenNote {
                            path: d.path.clone()?,
                            revision: d.revision,
                            rope: d.editor_loaded.then(|| d.editor.read(cx).text().clone()),
                            saved: d.saved.clone(),
                            dirty: d.dirty,
                        })
                    })
                    .collect::<Vec<_>>();
                Some((moves, open))
            }) else {
                return;
            };
            let mut worker_open = open.clone();
            let worker_root = root.clone();
            let worker_index = index.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut index = worker_index.lock().unwrap();
                    let changes = index.observe(&worker_root, moves);
                    for note in &mut worker_open {
                        note.path = links::moved(&note.path, &changes.external_moves);
                    }
                    let mut planned = std::collections::BTreeMap::new();
                    if changes.moves.is_empty() {
                        let mut changed = index.refresh(&worker_root);
                        for note in &worker_open {
                            if note.dirty && !index.has_open_revision(&note.path, note.revision) {
                                index.update_open_revision(&note.path, &note.text(), note.revision);
                                changed = true;
                            }
                        }
                        if changed {
                            index.rebuild_open();
                        }
                    } else {
                        let dirty = worker_open
                            .iter()
                            .filter(|n| n.dirty)
                            .map(|n| (n.path.clone(), n.text()))
                            .collect();
                        let (rules, repairs) = index.prepare(&worker_root, changes.moves, &dirty);
                        let affected = repairs.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
                        for (path, patches) in repairs {
                            planned.insert(
                                path.clone(),
                                links::Repair {
                                    path,
                                    patches,
                                    rules: rules.clone(),
                                },
                            );
                        }
                        index.finish_moves(&rules, &affected);
                    }
                    if !changes.copies.is_empty() {
                        let copies = changes.copies;
                        let copy_repairs = index.prepare_copies(&worker_root, copies.clone());
                        let rules = index.copy_rules(&copies);
                        for (path, patches) in copy_repairs {
                            planned
                                .entry(path.clone())
                                .or_insert_with(|| links::Repair {
                                    path,
                                    patches,
                                    rules: rules.clone(),
                                });
                        }
                    }
                    for repair in index.recover_paths(&worker_root, &changes.changed_notes) {
                        planned.entry(repair.path.clone()).or_insert(repair);
                    }
                    let affected = planned.keys().cloned().collect::<Vec<_>>();
                    let mut live = Vec::new();
                    let mut errors = Vec::new();
                    for (_, repair) in planned {
                        if let Some(note) = worker_open
                            .iter()
                            .find(|n| n.path == repair.path && n.rope.is_some())
                        {
                            live.push((note.clone(), repair.patches, repair.rules));
                        } else if let Err(error) =
                            links::write_patches(&repair.path, &repair.patches)
                        {
                            errors.push(format!("{}: {error}", repair.path.display()));
                        }
                    }
                    index.refresh_paths(&affected);
                    (index.snapshot(), live, errors, changes.external_moves)
                })
                .await;
            let (mut snapshot, repairs, errors, external_moves) = result;
            if !external_moves.is_empty() {
                let _ = this.update(cx, |app, cx| {
                    if app.root.as_ref() == Some(&root) {
                        for movement in external_moves {
                            app.follow_entry_move(&movement.old, &movement.new, cx);
                        }
                        cx.notify();
                    }
                });
            }
            let mut updated_open = Vec::new();
            // A worker never overwrites an editor snapshot that the user changed.
            // Retry that note against the same pre-move catalogue on a worker.
            {
                for (mut note, mut patches, rules) in repairs {
                    loop {
                        let worker_note = note.clone();
                        let worker_patches = patches.clone();
                        let prepared = cx
                            .background_executor()
                            .spawn(async move {
                                let source = Arc::new(worker_note.text());
                                let updated = links::apply(&source, &worker_patches)?;

                                let first = worker_patches.first()?.range.start;
                                let last = worker_patches.last()?.range.end;
                                let replacement_end = updated.len() - (source.len() - last);
                                let replacement = updated[first..replacement_end].to_owned();
                                Some((source, Arc::new(updated), replacement))
                            })
                            .await;
                        let Some((_source, updated, replacement)) = prepared else {
                            break;
                        };
                        let retry = handle
                            .update(cx, |_, window, cx| {
                                this.update(cx, |app, cx| {
                                    if app.root.as_ref() != Some(&root) {
                                        return None;
                                    }
                                    let Some(i) = app
                                        .documents
                                        .iter()
                                        .position(|d| d.path.as_ref() == Some(&note.path))
                                    else {
                                        return None;
                                    };
                                    let doc = &mut app.documents[i];
                                    if doc.revision != note.revision {
                                        return Some(OpenNote {
                                            path: note.path.clone(),
                                            revision: doc.revision,
                                            rope: doc
                                                .editor_loaded
                                                .then(|| doc.editor.read(cx).text().clone()),
                                            saved: doc.saved.clone(),
                                            dirty: doc.dirty,
                                        });
                                    }
                                    if doc.editor_loaded {
                                        // One undoable edit, with the existing selection retained.
                                        let first = patches.first().unwrap().range.start;
                                        let last = patches.last().unwrap().range.end;
                                        let selection = doc.editor.read(cx).selected_range();
                                        let editor = doc.editor.clone();
                                        editor.update(cx, |e, cx| {
                                            e.set_selected_range(first..last, cx);
                                            e.replace(replacement, window, cx);
                                            let shifted =
                                                |at: usize| links::shift_offset(at, &patches);
                                            e.set_selected_range(
                                                shifted(selection.start)..shifted(selection.end),
                                                cx,
                                            );
                                        });
                                    }
                                    cx.notify();
                                    None
                                })
                            })
                            .ok()
                            .and_then(Result::ok)
                            .flatten();
                        let Some(next) = retry else {
                            updated_open.push((note.path.clone(), updated));
                            break;
                        };
                        note = next;
                        let worker_rules = rules.clone();
                        let worker_note = note.clone();
                        patches = cx
                            .background_executor()
                            .spawn(async move {
                                worker_rules.patches(
                                    &links::original(&worker_note.path, &worker_rules.moves),
                                    &links::scan(&worker_note.text()),
                                )
                            })
                            .await;
                        if patches.is_empty() {
                            break;
                        }
                    }
                }
            }
            if !updated_open.is_empty() {
                snapshot = cx
                    .background_executor()
                    .spawn(async move {
                        let mut index = index.lock().unwrap();
                        for (path, text) in updated_open {
                            index.update_open(&path, &text);
                        }
                        index.rebuild_open();
                        index.snapshot()
                    })
                    .await;
            }
            let _ = this.update(cx, |app, cx| {
                if app.root.as_ref() == Some(&root) {
                    app.link_snapshot = snapshot;
                }
                app.repairing_links = false;
                app.link_task = None;
                for error in errors {
                    app.message(format!("Could not update linked paths: {error}"), true, cx);
                }
                for i in 0..app.documents.len() {
                    if app.documents[i].dirty {
                        app.schedule_save(i, cx);
                    }
                }
                if !app.link_moves.is_empty() {
                    app.refresh_links(cx);
                } else {
                    for i in (0..app.documents.len()).rev() {
                        let d = &app.documents[i];
                        if d.close_after_save && !d.dirty && !d.saving {
                            app.remove_document(i, cx);
                        }
                    }
                    if let Some(target) = app.closing
                        && app.documents.iter().all(|d| !d.dirty && !d.saving)
                    {
                        app.finish_close(target, cx);
                    }
                }
                cx.notify();
            });
        }));
    }
    pub(crate) fn wiki_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self
            .active
            .map(|i| &self.documents[i])
            .filter(|d| d.markdown && !d.locked && d.editor_loaded && self.mode != Mode::Read)
        else {
            self.wiki_completion = None;
            return;
        };
        if self.palette.is_some()
            || self.controls.is_some()
            || !doc.editor.focus_handle(cx).is_focused(window)
        {
            self.wiki_completion = None;
            return;
        }
        let editor = doc.editor.read(cx);
        let range = editor.selected_range();
        if !range.is_empty() {
            self.wiki_completion = None;
            return;
        }
        // Read only a short slice before the caret, even in a multi-megabyte file.
        let start = range.start.saturating_sub(260);
        let rope = editor.text();
        let mut start = start;
        while start < range.start && !rope.is_char_boundary(start) {
            start += 1;
        }
        let Ok(slice) = rope.try_slice(start..range.start) else {
            return;
        };
        let text = slice.to_string();
        let Some(open) = text.rfind("[[") else {
            self.wiki_completion = None;
            return;
        };
        let query = &text[open + 2..];
        if query.contains(['\n', '\r', ']', '[', '|', '#', '`'])
            || (open > 0 && text.as_bytes()[open - 1] == b'\\')
        {
            self.wiki_completion = None;
            return;
        }
        let Some(path) = doc.path.as_ref() else {
            return;
        };
        let id = doc.editor.entity_id();
        let at = start + open;
        if self.wiki_completion.as_ref().is_some_and(|c| {
            c.editor == id
                && c.start == at
                && c.end == range.start
                && c.query == query
                && Arc::ptr_eq(&c.catalog, &self.link_snapshot.catalog)
        }) {
            return;
        }
        let dismissed = self
            .wiki_completion
            .as_ref()
            .is_some_and(|c| c.editor == id && c.start == at && c.dismissed);
        let anchor = editor
            .range_to_bounds(&(at..at))
            .map(|b| point(b.left(), b.bottom() + px(6.)))
            .or_else(|| {
                editor
                    .cursor_layout()
                    .map(|(b, _)| point(b.left(), b.bottom() + px(6.)))
            })
            .unwrap_or(point(px(100.), px(100.)));
        let choices = self.link_snapshot.catalog.suggestions(path, query);
        self.wiki_completion = Some(Completion {
            editor: id,
            start: at,
            end: range.start,
            query: query.into(),
            anchor,
            selected: 0,
            choices,
            dismissed,
            catalog: self.link_snapshot.catalog.clone(),
            scroll: ScrollHandle::new(),
        });
    }
    pub(crate) fn wiki_is_open(&self) -> bool {
        self.wiki_completion.as_ref().is_some_and(|c| !c.dismissed)
    }
    pub(crate) fn wiki_accept(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self
            .wiki_completion
            .as_ref()
            .filter(|c| !c.dismissed && !c.choices.is_empty())
        {
            self.confirm_wiki(c.selected, w, cx);
        }
    }
    pub(crate) fn wiki_navigate(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(c) = self
            .wiki_completion
            .as_mut()
            .filter(|c| !c.choices.is_empty())
        {
            c.selected =
                (c.selected as isize + delta).rem_euclid(c.choices.len() as isize) as usize;
            c.scroll.scroll_to_item(c.selected);
            cx.notify();
        }
    }
    pub(crate) fn wiki_dismiss(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.wiki_completion.as_mut() {
            c.dismissed = true;
            cx.notify();
        }
    }
    fn confirm_wiki(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.wiki_completion.take() else {
            return;
        };
        let Some(target) = c.choices.get(row) else {
            return;
        };
        let Some(doc) = self
            .documents
            .iter()
            .find(|d| d.editor.entity_id() == c.editor && !d.locked)
        else {
            return;
        };
        let Some(path) = &doc.path else { return };
        let name = self.link_snapshot.catalog.destination(path, &target.path);
        let mut end = c.end;
        if doc
            .editor
            .read(cx)
            .text()
            .try_slice(end..(end + 2).min(doc.editor.read(cx).text().len()))
            .ok()
            .is_some_and(|s| s.to_string() == "]]")
        {
            end += 2;
        }
        doc.editor.update(cx, |e, cx| {
            e.set_selected_range(c.start..end, cx);
            e.replace(format!("[[{name}]]"), window, cx);
        });
        cx.notify();
    }
    pub(crate) fn wiki_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.wiki_completion.as_ref().filter(|c| !c.dismissed)?;
        let theme = cx.theme().clone();
        let size = window.viewport_size();
        let width = px(300.).min(size.width - px(24.));
        let height = px((c.choices.len().min(6) as f32 * 40. + 42.).min(282.));
        let left = c.anchor.x.min(size.width - width - px(12.)).max(px(12.));
        let top = if c.anchor.y + height > size.height - px(12.) {
            (c.anchor.y - height - px(26.)).max(px(12.))
        } else {
            c.anchor.y
        };
        let mut list = v_flex()
            .id("wiki-choices")
            .max_h(px(240.))
            .overflow_y_scroll()
            .track_scroll(&c.scroll)
            .p_1();
        if c.choices.is_empty() {
            list = list.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No matching notes"),
            );
        }
        for (i, note) in c.choices.iter().enumerate() {
            list = list.child(
                v_flex()
                    .id(("wiki-choice", i))
                    .debug_selector(move || format!("wiki-choice-{i}"))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(c.selected == i, |d| d.bg(theme.secondary))
                    .hover(|d| d.bg(theme.secondary))
                    .child(div().text_sm().child(note.name.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(note.label.clone()),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, w, cx| app.confirm_wiki(i, w, cx))),
            );
        }
        Some(
            div()
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .card_surface(&theme)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Link to Note"),
                )
                .child(list)
                .into_any_element(),
        )
    }
    pub(crate) fn follow_note_link(
        &mut self,
        editor_id: EntityId,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self
            .documents
            .iter()
            .find(|d| d.editor.entity_id() == editor_id)
            .and_then(|d| d.path.clone())
        else {
            return;
        };
        if let Some(value) = url.strip_prefix("enso-wiki:") {
            let value = links::decode(value);
            if let Some(path) = self.link_snapshot.catalog.resolve(&from, &value) {
                self.open_path(path, window, cx);
            } else {
                self.message(
                    format!("Note “{value}” could not be found in this workspace."),
                    true,
                    cx,
                );
            }
        } else if url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("mailto:")
        {
            cx.open_url(url);
        } else if let Some(path) = links::local_path(from.parent().unwrap_or(&from), url) {
            if files::is_markdown(&path) {
                self.open_path(path, window, cx);
            } else {
                cx.reveal_path(&path);
            }
        }
    }
    pub(crate) fn linked_from(&self, editor_id: EntityId, cx: &mut Context<Self>) -> AnyElement {
        let Some(path) = self
            .documents
            .iter()
            .find(|d| d.editor.entity_id() == editor_id)
            .and_then(|d| d.path.as_ref())
        else {
            return div().into_any_element();
        };
        let Some(sources) = self
            .link_snapshot
            .backlinks
            .get(path)
            .filter(|s| !s.is_empty())
        else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let mut list = v_flex().gap_1().pt_6().pb_4().w_full().child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("Linked from · {}", sources.len())),
        );
        for (i, source) in sources.iter().enumerate() {
            let path = source.clone();
            let label = source
                .strip_prefix(&self.link_snapshot.catalog.root)
                .unwrap_or(source)
                .to_string_lossy()
                .to_string();
            list = list.child(
                h_flex()
                    .id(("backlink", i))
                    .debug_selector(move || format!("backlink-{i}"))
                    .gap_2()
                    .py_1()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .hover(|d| d.text_color(theme.foreground))
                    .child(Icon::new(IconName::Link).size_3())
                    .child(label)
                    .on_click(cx.listener(move |app, _, w, cx| app.open_path(path.clone(), w, cx))),
            );
        }
        list.into_any_element()
    }
}

/// Rewritten blocks keep the surrounding Markdown structure and wrapping.
/// Custom blocks preserve the outer source span; images/tables inside use the
/// explicit source map to write edits back into the original note.
#[derive(Clone)]
struct WikiBlock {
    text: String,
    map: Arc<SourceMap>,
}
#[derive(Clone, Default)]
pub struct SourceMap {
    pub offset: usize,
    pub prefix: usize,
    pub edits: Vec<(Range<usize>, Range<usize>)>,
    pub parent: Option<Arc<SourceMap>>,
}
impl SourceMap {
    pub fn offset(offset: usize, prefix: usize) -> Arc<Self> {
        Arc::new(Self {
            offset,
            prefix,
            ..Default::default()
        })
    }
    pub fn point(&self, at: usize) -> Option<usize> {
        let mut before = 0;
        let mut after = 0;
        for (generated, source) in &self.edits {
            if at < generated.start {
                break;
            }
            if at < generated.end {
                return self.finish(source.start);
            }
            before = generated.end;
            after = source.end;
        }
        self.finish(at.checked_sub(before)? + after)
    }
    fn finish(&self, at: usize) -> Option<usize> {
        let at = at.checked_sub(self.prefix)? + self.offset;
        self.parent.as_ref().map_or(Some(at), |p| p.point(at))
    }
    pub fn range(&self, range: Range<usize>) -> Option<Range<usize>> {
        Some(self.point(range.start)?..self.point(range.end)?)
    }
    pub fn child(self: &Arc<Self>, offset: usize) -> Arc<Self> {
        Arc::new(Self {
            offset,
            parent: Some(self.clone()),
            ..Default::default()
        })
    }
}
pub struct WikiBlocks {
    pub owner: WeakEntity<Enso>,
    pub editor: EntityId,
    pub base: Option<PathBuf>,
    pub mapping: Arc<SourceMap>,
}
impl MarkdownPlugin for WikiBlocks {
    fn is_block(&self) -> bool {
        true
    }
    fn name(&self) -> &str {
        "enso-wiki-block"
    }
    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        if !matches!(node, Node::Paragraph(_) | Node::Heading(_) | Node::Table(_)) {
            return None;
        }
        let source = cx.node_source(node)?;
        let (text, edits) = render_source(source)?;
        let offset = node.position()?.start.offset;
        let map = Arc::new(SourceMap {
            offset,
            prefix: 0,
            edits,
            parent: Some(self.mapping.clone()),
        });
        Some(
            MarkdownNode::new(self.name(), WikiBlock { text, map })
                .text(node.to_string())
                .markdown(source.to_owned()),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let block = node.data::<WikiBlock>().unwrap();
        let owner = self.owner.clone();
        let editor = self.editor;
        let base = self.base.clone();
        crate::rich::decorate_mapped(
            TextView::markdown(
                ("wiki-block", node.source_range().map_or(0, |r| r.start)),
                block.text.clone(),
            ),
            self.owner.clone(),
            editor,
            base.clone(),
            block.map.clone(),
        )
        .w_full()
        .selectable(true)
        .map(|view| typography::style(view, cx))
        .image_source(move |uri| {
            let s = uri.to_string();
            if !s.contains(':')
                && let Some(base) = &base
            {
                return ImageSource::from(base.join(links::decode(&s)));
            }
            ImageSource::from(uri.clone())
        })
        .on_link_click(move |url, event, w, cx| {
            if !event.is_right_click() {
                let _ = owner.update(cx, |app, cx| app.follow_note_link(editor, url, w, cx));
            }
        })
    }
}
pub fn render_source(source: &str) -> Option<(String, Vec<(Range<usize>, Range<usize>)>)> {
    let links = links::scan(source);
    let mut at = 0;
    let mut out = String::new();
    let mut edits = Vec::new();
    for link in links.into_iter().filter(|l| l.kind == links::Kind::Wiki) {
        let start = link.full.start;
        let end = link.full.end;
        if start < at {
            continue;
        }
        out.push_str(&source[at..start]);
        let generated_start = out.len();
        let inner = &source[start + 2..end - 2];
        let label = inner.split_once('|').map_or_else(
            || link.value.split('#').next().unwrap_or(&link.value),
            |(_, label)| label,
        );
        let label = label
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('*', "\\*")
            .replace('_', "\\_")
            .replace('`', "\\`");
        out.push_str(&format!(
            "[{label}](enso-wiki:{})",
            links::encode(&link.value)
        ));
        edits.push((generated_start..out.len(), start..end));
        at = end;
    }
    if edits.is_empty() {
        return None;
    }
    out.push_str(&source[at..]);
    Some((out, edits))
}

pub fn preview_source(source: &str, workspace: bool) -> String {
    if workspace {
        format!("{source}\n\n<!--enso-backlinks-->\n")
    } else {
        source.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn wiki_blocks_wrap_normally_and_map_controls_after_links_to_original_bytes() {
        let source = "## [[  Note#section |My note]] and <img src=\"photo.png\" />";
        let (rendered, edits) = render_source(source).unwrap();
        assert!(
            rendered.contains("[My note](enso-wiki:Note%23section)"),
            "{rendered}"
        );
        let map = SourceMap {
            offset: 120,
            prefix: 0,
            edits,
            parent: None,
        };
        let generated = rendered.find("<img").unwrap()..rendered.len();
        assert_eq!(
            map.range(generated),
            Some(120 + source.find("<img").unwrap()..120 + source.len())
        );
        assert!(render_source("`[[code]]`\n\n```md\n[[code]]\n```").is_none());
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::*;
    use ::core::prelude::v1::test;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, WindowOptions};
    fn draw(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    #[gpui_kit::test]
    fn typing_double_brackets_suggests_notes_and_enter_inserts_without_a_newline(
        cx: &mut TestAppContext,
    ) {
        let root = std::env::temp_dir().join(format!("enso-wiki-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let from = root.join("Start.md");
        let target = root.join("Notes.md");
        std::fs::write(&from, "").unwrap();
        std::fs::write(&target, "# Notes\n").unwrap();
        let mut index = links::Index::default();
        index.refresh(&root);
        let snapshot = index.snapshot();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.root = Some(root.clone());
                    app.link_snapshot = snapshot;
                    app.add_document(Some(from.clone()), "".into(), None, w, cx);
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| editor.focus_handle(cx).focus(w, cx))
            .unwrap();
        cx.simulate_input(handle, "[[No");
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            let c = app.wiki_completion.as_ref().expect("wiki suggestions");
            assert_eq!(c.choices.len(), 1);
            assert_eq!(c.choices[0].path, target);
        });
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        assert_eq!(
            cx.update(|cx| editor.read(cx).value().to_string()),
            "[[Notes.md]]"
        );
        cx.simulate_input(handle, " and [[");
        draw(cx, handle);
        let mut visual = VisualTestContext::from_window(handle, cx);
        let choice = visual
            .debug_bounds("wiki-choice-0")
            .expect("clickable note suggestion");
        visual.simulate_click(choice.center(), Modifiers::default());
        draw(cx, handle);
        assert_eq!(
            cx.update(|cx| editor.read(cx).value().to_string()),
            "[[Notes.md]] and [[Notes.md]]"
        );
        // A wiki link follows the same open-note path as the explorer.
        cx.update_window(handle, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.follow_note_link(editor.entity_id(), "enso-wiki:Notes", w, cx)
            })
        })
        .unwrap();
        for _ in 0..4 {
            draw(cx, handle);
        }
        assert!(cx.update(|cx| {
            view.read(cx)
                .documents
                .iter()
                .any(|d| d.path.as_ref() == Some(&target))
        }));
        // The read footer is an actual scrollable block and opens its source.
        cx.update(|cx| {
            view.update(cx, |app, cx| {
                let mut backlinks = HashMap::new();
                backlinks.insert(target.clone(), vec![from.clone()]);
                app.link_snapshot.backlinks = Arc::new(backlinks);
                cx.notify();
            })
        });
        for _ in 0..3 {
            draw(cx, handle);
        }
        let footer = visual
            .debug_bounds("backlink-0")
            .expect("Linked from at the end of the note");
        visual.simulate_click(footer.center(), Modifiers::default());
        draw(cx, handle);
        assert_eq!(
            cx.update(|cx| view.read(cx).documents[view.read(cx).active.unwrap()]
                .path
                .clone()),
            Some(from)
        );
        let _ = std::fs::remove_dir_all(root);
    }
    #[gpui_kit::test]
    fn filesystem_arrivals_and_moves_reconcile_without_copy_or_drop_handlers(
        cx: &mut TestAppContext,
    ) {
        let root = std::env::temp_dir().join(format!("enso-observer-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lol")).unwrap();
        std::fs::create_dir_all(root.join(".assets/images")).unwrap();
        image::RgbImage::new(1, 1)
            .save(root.join(".assets/images/photo.jpg"))
            .unwrap();
        let source = "<img src=\".assets/images/photo.jpg\" alt=\"photo\" />\n\n[^1]: The footnote's text.\n";
        let old = root.join("markdown-features 2.md");
        let copied = root.join("lol/markdown-features 2.md");
        std::fs::write(&old, source).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.root = Some(root.clone());
                    app.add_document(Some(old.clone()), source.into(), None, w, cx);
                    app.refresh_links(cx);
                    app
                })
            })
            .unwrap()
        });
        let settle = |cx: &mut TestAppContext| {
            for _ in 0..22 {
                cx.executor().advance_clock(Duration::from_millis(100));
                draw(cx, handle);
            }
        };
        settle(cx);
        // A file-manager copy: no enso copy, paste, drop or repair action.
        std::fs::copy(&old, &copied).unwrap();
        settle(cx);
        assert_eq!(
            std::fs::read_to_string(&copied).unwrap(),
            source.replace("src=\".assets/", "src=\"../.assets/")
        );
        assert_eq!(std::fs::read_to_string(&old).unwrap(), source);
        // A synced note with no original or equal-content match.
        let synced = root.join("lol/Synced.md");
        std::fs::write(
            &synced,
            "# From another device\n\n<img src=\".assets/images/photo.jpg\" />",
        )
        .unwrap();
        settle(cx);
        assert!(
            std::fs::read_to_string(&synced)
                .unwrap()
                .contains("src=\"../.assets/images/photo.jpg\"")
        );
        // An OS move of an open note: tab follows it and relative links rebase.
        let relocated = root.join("lol/Relocated.md");
        std::fs::rename(&old, &relocated).unwrap();
        settle(cx);
        assert_eq!(
            std::fs::read_to_string(&relocated).unwrap(),
            source.replace("src=\".assets/", "src=\"../.assets/")
        );
        assert_eq!(
            cx.update(|cx| view.read(cx).documents[0].path.clone()),
            Some(relocated)
        );
        // An OS asset rename repairs references in open and closed notes.
        std::fs::rename(
            root.join(".assets/images/photo.jpg"),
            root.join(".assets/images/renamed.jpg"),
        )
        .unwrap();
        settle(cx);
        assert!(
            std::fs::read_to_string(&copied)
                .unwrap()
                .contains("../.assets/images/renamed.jpg")
        );
        assert!(
            std::fs::read_to_string(&synced)
                .unwrap()
                .contains("../.assets/images/renamed.jpg")
        );
        cx.update_window(handle, |_, w, _| w.remove_window())
            .unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
    #[gpui_kit::test]
    fn moving_a_note_into_lol_rebases_images_in_read_edit_and_large_read_modes(
        cx: &mut TestAppContext,
    ) {
        let tag = r#"<img src=".assets/images/photo.jpg" alt="photo" width="36.6%" style="display: block; margin-left: auto; margin-right: auto;" />"#;
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        for (case, large, editing) in [
            ("read", false, false),
            ("edit", false, true),
            ("large", true, false),
        ] {
            let root = std::env::temp_dir()
                .join(format!("enso-note-rebase-{}-{case}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("lol")).unwrap();
            std::fs::create_dir_all(root.join(".assets/images")).unwrap();
            image::RgbImage::new(1, 1)
                .save(root.join(".assets/images/photo.jpg"))
                .unwrap();
            let source = format!(
                "{tag}\n\n{}",
                if large {
                    "Paragraph.\n\n".repeat(25_000)
                } else {
                    String::new()
                }
            );
            let old = root.join("big.md");
            let new = root.join("lol/big.md");
            std::fs::write(&old, &source).unwrap();
            let sections = large.then(|| reader::LargeReader::index(&source));
            let (handle, view) = cx.update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                    cx.new(|cx| {
                        let mut app = Enso::new(w, cx);
                        app.root = Some(root.clone());
                        app.add_document(Some(old.clone()), source.clone(), sections, w, cx);
                        if editing {
                            app.mode = Mode::Edit;
                        }
                        app
                    })
                })
                .unwrap()
            });
            draw(cx, handle);
            if editing {
                let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
                cx.update_window(handle, |_, w, cx| {
                    editor.focus_handle(cx).focus(w, cx);
                    editor.update(cx, |e, cx| {
                        e.set_selected_range(source.len()..source.len(), cx)
                    });
                })
                .unwrap();
                cx.simulate_input(handle, "Unsaved words.");
            }
            cx.update(|cx| {
                view.update(cx, |app, cx| {
                    app.move_entry(old.clone(), root.join("lol"), cx);
                })
            });
            for _ in 0..25 {
                cx.executor().advance_clock(Duration::from_millis(100));
                draw(cx, handle);
            }
            let expected = source.replace("src=\".assets/", "src=\"../.assets/")
                + if editing { "Unsaved words." } else { "" };
            assert_eq!(
                std::fs::read_to_string(&new).unwrap(),
                expected,
                "{case}: saved note keeps the original image target"
            );
            cx.update(|cx| {
                let app = view.read(cx);
                let doc = &app.documents[0];
                assert_eq!(doc.path.as_ref(), Some(&new));
                if large {
                    assert!(
                        !doc.editor_loaded,
                        "repair must preserve lazy editor loading"
                    );
                    assert_eq!(doc.saved.as_str(), expected);
                    assert_eq!(
                        doc.large_reader
                            .as_ref()
                            .unwrap()
                            .read(cx)
                            .source()
                            .as_str(),
                        expected
                    );
                } else {
                    assert_eq!(doc.editor.read(cx).value().as_str(), expected);
                }
            });
            cx.update_window(handle, |_, w, _| w.remove_window())
                .unwrap();
            let _ = std::fs::remove_dir_all(root);
        }
    }
    #[gpui_kit::test]
    fn moving_an_image_repairs_closed_and_unsaved_open_notes(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("enso-wiki-move-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("images")).unwrap();
        let from = root.join("Open.md");
        let closed = root.join("Closed.md");
        let image = root.join("photo.png");
        let source = "<img src=\"photo.png\" />\n";
        std::fs::write(&from, source).unwrap();
        std::fs::write(&closed, "![photo](photo.png)").unwrap();
        std::fs::write(&image, b"image").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.root = Some(root.clone());
                    app.add_document(Some(from.clone()), source.into(), None, w, cx);
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, w, cx| {
            editor.focus_handle(cx).focus(w, cx);
            editor.update(cx, |e, cx| {
                e.set_selected_range(source.len()..source.len(), cx)
            });
        })
        .unwrap();
        cx.simulate_input(handle, "Unsaved words stay here.");
        cx.update(|cx| {
            view.update(cx, |app, cx| {
                app.move_entry(image.clone(), root.join("images"), cx);
            })
        });
        for _ in 0..12 {
            cx.executor().advance_clock(Duration::from_millis(100));
            draw(cx, handle);
        }
        let text = cx.update(|cx| editor.read(cx).value().to_string());
        assert_eq!(
            text,
            "<img src=\"images/photo.png\" />\nUnsaved words stay here."
        );
        assert_eq!(
            std::fs::read_to_string(&closed).unwrap(),
            "![photo](images/photo.png)"
        );
        assert!(!cx.update(|cx| view.read(cx).repairing_links));
        let _ = std::fs::remove_dir_all(root);
    }
}
