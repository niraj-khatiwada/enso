mod app_icon;
mod brand;
mod files;
mod onboarding;
mod reader;
mod settings;
mod stats;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Transition, transition};
#[cfg(all(test, feature = "ui-tests"))]
extern crate gpui_kit as gpui;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Editor, EditorState, Input, InputEvent, InputState},
    resizable::{HandleEdge, h_resizable, resizable_panel, resize_handle},
    text::{RenderedText, TextView, TextViewState},
    v_flex,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

actions!(
    enso,
    [
        OpenFile,
        OpenSettings,
        OpenFolder,
        NewFile,
        CloseFile,
        Save,
        ToggleView,
        ToggleSplit,
        Quit
    ]
);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Read,
    Edit,
    Split,
}

struct Document {
    path: Option<PathBuf>,
    editor: Entity<EditorState>,
    preview: Entity<TextViewState>,
    saved: Arc<String>,
    dirty: bool,
    editor_loaded: bool,
    large_reader: Option<Entity<reader::LargeReader>>,
    stats: String,
    _stats_task: Option<Task<()>>,
    last_reader_cursor: Option<usize>,
    last_preview_text: Option<RenderedText>,
    _preview_task: Option<Task<()>>,
    _autosave_timer: Option<Task<()>>,
    revision: u64,
    saving: bool,
    close_after_save: bool,
    markdown: bool,
    _subscriptions: Vec<Subscription>,
}
impl Document {
    fn dirty(&self, _: &App) -> bool {
        self.dirty
    }
    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.md".into())
    }
}

#[derive(Clone)]
struct SidebarResize {
    width: Pixels,
    pointer_x: Pixels,
}
impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Enso {
    focus: FocusHandle,
    documents: Vec<Document>,
    active: Option<usize>,
    root: Option<PathBuf>,
    folders: HashMap<PathBuf, Vec<files::Entry>>,
    expanded: HashSet<PathBuf>,
    mode: Mode,
    previous_mode: Mode,
    status: String,
    error: bool,
    naming: bool,
    filename: Entity<InputState>,
    closing: bool,
    sidebar_open: bool,
    sidebar_width: Pixels,
    focus_after_close: bool,
    hovered_tab: Option<EntityId>,
    _subscriptions: Vec<Subscription>,
}

impl Enso {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("Enso");
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let filename = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled.md"));
        let subscription = cx.subscribe_in(&filename, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.create_file(window, cx);
            }
        });
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.documents.iter().any(|d| d.dirty(cx)) {
                    this.request_quit(window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        Self {
            focus,
            documents: Vec::new(),
            active: None,
            root: None,
            folders: HashMap::new(),
            expanded: HashSet::new(),
            mode: Mode::Read,
            previous_mode: Mode::Read,
            status: "A little space for your thoughts.".into(),
            error: false,
            naming: false,
            filename,
            closing: false,
            sidebar_open: true,
            sidebar_width: px(248.),
            focus_after_close: false,
            hovered_tab: None,
            _subscriptions: vec![subscription, settings::watch_system(window)],
        }
    }

    fn open_first_launch_sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async { onboarding::load() });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Some((path, source))) => {
                    // A file opened while initialization was pending wins focus.
                    let active = this.active;
                    let mode = this.mode;
                    this.add_document(Some(path), source, None, window, cx);
                    if active.is_some() {
                        this.active = active;
                        this.mode = mode;
                    }
                }
                Ok(None) => {}
                Err(error) => this.message(
                    format!("Could not create Get Started.md: {error}"),
                    true,
                    cx,
                ),
            });
        })
        .detach();
    }

    fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active else {
            return;
        };
        if !self.documents[index].markdown {
            return;
        }
        if mode != Mode::Read && !self.documents[index].editor_loaded {
            let source = self.documents[index].saved.clone();
            self.documents[index].editor.update(cx, |editor, cx| {
                editor.set_value(source.as_str(), window, cx)
            });
            self.documents[index].editor_loaded = true;
        }
        if mode == Mode::Split && self.mode != Mode::Split {
            self.previous_mode = self.mode;
        }
        self.mode = mode;
        // Hidden reader entities can retain a frame measured in a different
        // pane. Invalidate layout when bringing them back without reparsing
        // the complete document or resetting their own scroll position.
        if mode != Mode::Edit {
            if let Some(reader) = &self.documents[index].large_reader {
                reader.update(cx, |reader, cx| reader.refresh_layout(cx));
            } else {
                self.documents[index]
                    .preview
                    .update(cx, |preview, cx| preview.invalidate_inline_layout(cx));
            }
            self.reveal_editor_cursor(index, cx);
        }
        let focus = if mode == Mode::Read {
            self.focus.clone()
        } else {
            self.documents[index].editor.focus_handle(cx)
        };
        window.defer(cx, move |window, cx| focus.focus(window, cx));
        cx.notify();
    }

    fn reveal_editor_cursor(&mut self, index: usize, cx: &mut Context<Self>) {
        let doc = &mut self.documents[index];
        if !doc.markdown || !doc.editor_loaded {
            return;
        }
        let cursor = doc.editor.read(cx).cursor();
        doc.last_reader_cursor = Some(cursor);
        if let Some(reader) = &doc.large_reader {
            reader.update(cx, |reader, cx| reader.reveal_source(cursor, cx));
        } else {
            let rendered = doc.preview.read(cx).rendered_text();
            doc.last_preview_text = Some(rendered.clone());
            let source = rendered.source();
            let mut cursor = cursor.min(source.len());
            while !source.is_char_boundary(cursor) {
                cursor -= 1;
            }
            let start = source[..cursor].rfind('\n').map_or(0, |offset| offset + 1);
            // If the caret is on an empty/markup-only line, try the next
            // rendered line so the preview still follows the surrounding text.
            let mut offset = start;
            let mut target = None;
            for line in source[start..].split_inclusive('\n') {
                let end = offset + line.len();
                if let Some(range) = rendered.range_for_source(offset..end) {
                    target = Some(range);
                    break;
                }
                offset = end;
            }
            // A trailing blank line has no rendered span; show the nearest
            // preceding passage rather than leaving the preview elsewhere.
            let mut end = start.saturating_sub(1);
            while target.is_none() && end > 0 {
                let begin = source[..end].rfind('\n').map_or(0, |offset| offset + 1);
                target = rendered.range_for_source(begin..end);
                end = begin.saturating_sub(1);
            }
            if let Some(range) = target {
                doc.preview.update(cx, |preview, cx| {
                    let _ = preview.reveal_range(range, cx);
                });
            }
        }
    }

    fn schedule_word_count(&mut self, index: usize, immediate: bool, cx: &mut Context<Self>) {
        let editor_id = self.documents[index].editor.entity_id();
        self.documents[index]._stats_task = Some(cx.spawn(async move |this, cx| {
            if !immediate {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
            }
            let Ok(Some((revision, rope, saved))) = this.read_with(cx, |this, cx| {
                this.documents
                    .iter()
                    .find(|doc| doc.editor.entity_id() == editor_id)
                    .map(|doc| {
                        (
                            doc.revision,
                            doc.editor_loaded
                                .then(|| doc.editor.read(cx).text().clone()),
                            doc.saved.clone(),
                        )
                    })
            }) else {
                return;
            };
            let count = cx
                .background_executor()
                .spawn(async move {
                    if let Some(rope) = rope {
                        stats::word_count(rope.chars())
                    } else {
                        stats::word_count(saved.chars())
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(doc) = this
                    .documents
                    .iter_mut()
                    .find(|doc| doc.editor.entity_id() == editor_id && doc.revision == revision)
                {
                    doc.stats = stats::label(count);
                    cx.notify();
                }
            });
        }));
    }

    fn message(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.status = message.into();
        self.error = error;
        cx.notify();
    }

    fn add_document(
        &mut self,
        path: Option<PathBuf>,
        source: String,
        sections: Option<Vec<reader::Section>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .documents
            .iter()
            .position(|d| path.is_some() && d.path == path)
        {
            self.active = Some(index);
            cx.notify();
            return;
        }
        let markdown = path.as_ref().is_none_or(|p| files::is_markdown(p));
        let large = source.len() >= reader::LARGE_DOCUMENT_BYTES;
        let stats = if source.is_empty() {
            stats::label(0)
        } else {
            "Counting words…".into()
        };
        let source = Arc::new(source);
        let large_reader = sections.map(|sections| {
            cx.new(|_| {
                reader::LargeReader::new(
                    source.clone(),
                    sections,
                    path.as_ref().and_then(|p| p.parent()).map(PathBuf::from),
                )
            })
        });
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(if markdown && !large {
                    "markdown"
                } else {
                    "plaintext"
                })
                .folding(!large)
                .auto_close(!large)
                .line_number(true)
                .default_value(if large && markdown {
                    String::new()
                } else {
                    source.as_ref().clone()
                })
        });
        editor.update(cx, |editor, cx| editor.set_soft_wrap(!large, window, cx));
        let preview =
            cx.new(|cx| TextViewState::markdown(if large || !markdown { "" } else { &source }, cx));
        let preview_ref = preview.clone();
        let subscription = cx.subscribe(
            &editor,
            move |this: &mut Self,
                  editor: Entity<EditorState>,
                  event: &InputEvent,
                  cx: &mut Context<Self>| {
                if matches!(event, InputEvent::Change) {
                    if let Some(doc) = this
                        .documents
                        .iter_mut()
                        .find(|d| d.editor.entity_id() == editor.entity_id())
                    {
                        doc.dirty = true;
                        doc.revision += 1;
                        if markdown
                            && (large
                                || doc.large_reader.is_some()
                                || editor.read(cx).text().len() >= reader::LARGE_DOCUMENT_BYTES)
                        {
                            let editor = editor.clone();
                            let editor_id = editor.entity_id();
                            doc._preview_task = Some(cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(200))
                                    .await;
                                let value =
                                    editor.read_with(cx, |state, _| state.value().to_string());
                                let source = Arc::new(value);
                                let worker_source = source.clone();
                                let sections = cx
                                    .background_executor()
                                    .spawn(
                                        async move { reader::LargeReader::index(&worker_source) },
                                    )
                                    .await;
                                let _ = this.update(cx, |this, cx| {
                                    if let Some(doc) = this
                                        .documents
                                        .iter_mut()
                                        .find(|d| d.editor.entity_id() == editor_id)
                                    {
                                        if let Some(reader) = &doc.large_reader {
                                            reader.update(cx, |reader, cx| {
                                                reader.replace(source, sections, cx)
                                            });
                                        } else {
                                            let base = doc
                                                .path
                                                .as_ref()
                                                .and_then(|p| p.parent())
                                                .map(PathBuf::from);
                                            doc.large_reader = Some(cx.new(|_| {
                                                reader::LargeReader::new(source, sections, base)
                                            }));
                                        }
                                    }
                                });
                            }));
                        }
                        if markdown
                            && !large
                            && doc.large_reader.is_none()
                            && editor.read(cx).text().len() < reader::LARGE_DOCUMENT_BYTES
                        {
                            let value = editor.read(cx).value();
                            preview_ref.update(cx, |view, cx| view.set_text(&value, cx));
                        }
                    }
                    let editor_id = editor.entity_id();
                    if let Some(index) = this
                        .documents
                        .iter()
                        .position(|d| d.editor.entity_id() == editor_id)
                    {
                        this.schedule_save(index, cx);
                        this.schedule_word_count(index, false, cx);
                    }
                    cx.notify();
                }
            },
        );
        let cursor_subscription = cx.observe(&editor, |this, editor, cx| {
            if this.mode != Mode::Split {
                return;
            }
            let Some(index) = this.active else {
                return;
            };
            let doc = &this.documents[index];
            if doc.editor.entity_id() == editor.entity_id()
                && doc.last_reader_cursor != Some(editor.read(cx).cursor())
            {
                this.reveal_editor_cursor(index, cx);
            }
        });
        let preview_subscription = cx.observe(&preview, |this, preview, cx| {
            if this.mode != Mode::Split {
                return;
            }
            let Some(index) = this.active else {
                return;
            };
            let doc = &this.documents[index];
            if doc.preview.entity_id() == preview.entity_id()
                && doc.last_preview_text.as_ref() != Some(&preview.read(cx).rendered_text())
            {
                this.reveal_editor_cursor(index, cx);
            }
        });
        self.documents.push(Document {
            path,
            saved: source,
            dirty: false,
            editor_loaded: !large || !markdown,
            large_reader,
            stats,
            _stats_task: None,
            last_reader_cursor: None,
            last_preview_text: None,
            _preview_task: None,
            _autosave_timer: None,
            revision: 0,
            saving: false,
            close_after_save: false,
            editor,
            preview,
            markdown,
            _subscriptions: vec![subscription, cursor_subscription, preview_subscription],
        });
        self.active = Some(self.documents.len() - 1);
        self.schedule_word_count(self.documents.len() - 1, true, cx);
        self.mode = if markdown { Mode::Read } else { Mode::Edit };
        self.message("Ready", false, cx);
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.open_folder(path, cx);
            return;
        }
        if let Some(index) = self
            .documents
            .iter()
            .position(|d| d.path.as_ref() == Some(&path))
        {
            self.active = Some(index);
            cx.notify();
            return;
        }
        self.message("Opening file…", false, cx);
        let task = cx.background_executor().spawn(async move {
            files::read_text(&path).map(|text| {
                let sections =
                    if files::is_markdown(&path) && text.len() >= reader::LARGE_DOCUMENT_BYTES {
                        Some(reader::LargeReader::index(&text))
                    } else {
                        None
                    };
                (path, text, sections)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok((path, source, sections)) => {
                    this.add_document(Some(path), source, sections, window, cx)
                }
                Err(error) => this.message(error.to_string(), true, cx),
            });
        })
        .detach();
    }

    fn pick(&mut self, directory: bool, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !directory,
            directories: directory,
            multiple: false,
            prompt: Some(
                if directory {
                    "Open folder"
                } else {
                    "Open text file"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.open_path(path, window, cx);
                    }
                }
                Ok(Err(error)) => this.message(error.to_string(), true, cx),
                _ => {}
            });
        })
        .detach();
    }

    fn open_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.root = Some(path.clone());
        self.folders.clear();
        self.expanded.clear();
        self.expanded.insert(path.clone());
        self.load_folder(path, cx);
    }

    fn load_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let task = cx
            .background_executor()
            .spawn(async move { (path.clone(), files::list_folder(&path)) });
        cx.spawn(async move |this, cx| {
            let (path, result) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.root != root {
                    return;
                }
                match result {
                    Ok(entries) => {
                        this.folders.insert(path, entries);
                        cx.notify();
                    }
                    Err(error) => this.message(error.to_string(), true, cx),
                }
            });
        })
        .detach();
    }

    fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.root.is_some() {
            self.naming = true;
            self.filename
                .update(cx, |s, cx| s.set_value("Untitled.md", window, cx));
            self.filename.focus_handle(cx).focus(window, cx);
            cx.notify();
        } else {
            let folder = std::env::current_dir().unwrap_or_default();
            let receiver = cx.prompt_for_new_path(&folder, Some("Untitled.md"));
            cx.spawn_in(window, async move |this, cx| {
                let result = receiver.await;
                let _ = this.update_in(cx, |this, window, cx| match result {
                    Ok(Ok(Some(path))) => match files::save_as(&path, "") {
                        Ok(()) => {
                            this.add_document(Some(path), String::new(), None, window, cx);
                            this.mode = Mode::Edit;
                            if let Some(i) = this.active {
                                this.documents[i].editor.focus_handle(cx).focus(window, cx);
                            }
                        }
                        Err(error) => this.message(error.to_string(), true, cx),
                    },
                    Ok(Err(error)) => this.message(error.to_string(), true, cx),
                    _ => {}
                });
            })
            .detach();
        }
    }

    fn create_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.naming {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        match files::create_file(&root, &self.filename.read(cx).value()) {
            Ok(path) => {
                self.naming = false;
                self.load_folder(root, cx);
                self.add_document(Some(path), String::new(), None, window, cx);
                self.mode = Mode::Edit;
                if let Some(i) = self.active {
                    self.documents[i].editor.focus_handle(cx).focus(window, cx);
                }
            }
            Err(error) => self.message(error.to_string(), true, cx),
        }
    }

    fn close_current_file(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.active else {
            return;
        };
        self.close_document(index, cx);
    }

    fn close_document(&mut self, index: usize, cx: &mut Context<Self>) {
        let doc = &mut self.documents[index];
        if doc.dirty || doc.saving {
            doc.close_after_save = true;
            self.start_save(index, cx);
        } else {
            self.remove_document(index, cx);
        }
    }

    fn remove_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.hovered_tab == Some(self.documents[index].editor.entity_id()) {
            self.hovered_tab = None;
        }
        self.documents.remove(index);
        self.active = match self.active {
            _ if self.documents.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.documents.len() - 1)),
            other => other,
        };
        self.focus_after_close = true;
        cx.notify();
    }

    fn schedule_save(&mut self, index: usize, cx: &mut Context<Self>) {
        let editor_id = self.documents[index].editor.entity_id();
        self.documents[index]._autosave_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(index) = this
                    .documents
                    .iter()
                    .position(|doc| doc.editor.entity_id() == editor_id)
                {
                    this.start_save(index, cx);
                }
            });
        }));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.active {
            self.start_save(index, cx);
        }
    }

    fn start_save(&mut self, index: usize, cx: &mut Context<Self>) {
        let doc = &mut self.documents[index];
        if !doc.dirty || doc.saving {
            return;
        }
        let Some(path) = doc.path.clone() else {
            return;
        };
        let source = Arc::new(doc.editor.read(cx).value().to_string());
        let expected = doc.saved.clone();
        let revision = doc.revision;
        let editor_id = doc.editor.entity_id();
        doc.saving = true;
        let written = source.clone();
        let task = cx
            .background_executor()
            .spawn(async move { files::save(&path, &expected, &written) });
        // A write in progress is never cancelled by another keystroke. Newer
        // revisions queue behind it so the next conflict check sees our last save.
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = this
                    .documents
                    .iter()
                    .position(|doc| doc.editor.entity_id() == editor_id)
                else {
                    return;
                };
                let doc = &mut this.documents[index];
                doc.saving = false;
                match result {
                    Ok(()) => {
                        doc.saved = source;
                        doc.dirty = doc.revision != revision;
                        if doc.dirty {
                            this.schedule_save(index, cx);
                        } else if doc.close_after_save {
                            this.remove_document(index, cx);
                        }
                        this.message("", false, cx);
                        if this.closing && this.documents.iter().all(|d| !d.dirty && !d.saving) {
                            cx.quit();
                        }
                    }
                    Err(error) => {
                        doc.close_after_save = false;
                        this.closing = false;
                        this.message(format!("Auto-save failed: {error}"), true, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        if !self.documents.iter().any(|d| d.dirty(cx)) {
            cx.quit();
            return;
        }
        if !self.error {
            self.closing = true;
            for index in 0..self.documents.len() {
                self.start_save(index, cx);
            }
            return;
        }
        self.closing = true;
        let receiver = window.prompt(
            PromptLevel::Warning,
            "You have unsaved changes",
            Some("Automatic saving failed. Keep editing to preserve your changes, or discard and quit."),
            &["Keep editing", "Discard and quit"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = receiver.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.closing = false;
                if answer == Ok(1) {
                    cx.quit();
                }
            });
        })
        .detach();
    }

    fn tree_rows(&self, path: &PathBuf, depth: usize, rows: &mut Vec<(files::Entry, usize)>) {
        if let Some(entries) = self.folders.get(path) {
            for entry in entries {
                rows.push((entry.clone(), depth));
                if entry.directory && self.expanded.contains(&entry.path) {
                    self.tree_rows(&entry.path, depth + 1, rows);
                }
            }
        }
    }
}

impl Render for Enso {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_after_close {
            self.focus_after_close = false;
            let focus = self
                .active
                .filter(|index| self.mode != Mode::Read || !self.documents[*index].markdown)
                .map(|index| self.documents[index].editor.focus_handle(cx))
                .unwrap_or_else(|| self.focus.clone());
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }
        let mut rows = Vec::new();
        if let Some(root) = &self.root {
            self.tree_rows(root, 0, &mut rows);
        }
        let theme = cx.theme().clone();
        let motion = theme.motion_tokens();
        let sidebar_reveal = transition(
            "sidebar-reveal",
            if self.sidebar_open { 1.0_f32 } else { 0.0_f32 },
            Transition::new(motion.duration_normal).easing(motion.easing_move.clone()),
            window,
            cx,
        )
        .clamp(0., 1.);
        let sidebar_width = self.sidebar_width;

        let active_path = self.active.and_then(|i| self.documents[i].path.clone());
        let document_title = self
            .active
            .map(|i| self.documents[i].title())
            .unwrap_or_default();
        let markdown = self.active.is_none_or(|i| self.documents[i].markdown);
        let mode = if markdown { self.mode } else { Mode::Edit };
        let mut tabs = h_flex()
            .id("tabs")
            .gap_1()
            .px_5()
            .py_2()
            .when(cfg!(target_os = "macos"), |tabs| {
                tabs.px_0()
                    .py_0()
                    .h_full()
                    .items_center()
                    .flex_1()
                    .min_w_0()
            })
            .overflow_x_scroll();
        for (i, document) in self.documents.iter().enumerate() {
            let editor_id = document.editor.entity_id();
            let hovered = self.hovered_tab == Some(editor_id);
            let motion = theme.motion_tokens();
            let reveal = transition(
                ElementId::from(("tab-close-reveal", editor_id)),
                if hovered { 1.0_f32 } else { 0.0_f32 },
                Transition::new(motion.duration_fast).easing(motion.easing_move.clone()),
                window,
                cx,
            )
            .clamp(0., 1.);

            tabs = tabs.child(
                h_flex()
                    .id(("file-tab", editor_id))
                    .flex_shrink_0()
                    .items_center()
                    .rounded_md()
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        let next = if *hovered {
                            Some(editor_id)
                        } else if this.hovered_tab == Some(editor_id) {
                            None
                        } else {
                            this.hovered_tab
                        };
                        if this.hovered_tab != next {
                            this.hovered_tab = next;
                            cx.notify();
                        }
                    }))
                    .when(self.active == Some(i), |tab| tab.bg(theme.secondary))
                    .child(
                        Button::new(("tab", editor_id))
                            .small()
                            .ghost()
                            .label(format!(
                                "{}{}",
                                document.title(),
                                if document.dirty(cx) { " •" } else { "" }
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active = this
                                    .documents
                                    .iter()
                                    .position(|doc| doc.editor.entity_id() == editor_id);
                                cx.notify();
                            })),
                    )
                    .child(
                        h_flex()
                            .w(px(24. * reveal))
                            .h(px(28.))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .when(reveal <= 0.001, |slot| slot.invisible())
                            .child(
                                Button::new(("close-tab", editor_id))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::X)
                                    .tooltip("Close file")
                                    .accessibility_label(format!("Close {}", document.title()))
                                    .w(px(20.))
                                    .h(px(20.))
                                    .flex_shrink_0()
                                    .opacity(reveal)
                                    .disabled(!hovered)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(index) = this
                                            .documents
                                            .iter()
                                            .position(|doc| doc.editor.entity_id() == editor_id)
                                        {
                                            this.close_document(index, cx);
                                        }
                                    })),
                            ),
                    ),
            );
        }
        let sidebar = v_flex()
            .w_full()
            .h_full()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .when(!cfg!(target_os = "macos"), |sidebar| {
                sidebar.child(
                    h_flex()
                        .px_5()
                        .py_5()
                        .gap_3()
                        .items_center()
                        .child(brand::logo(26., cx))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("enso"),
                        ),
                )
            })
            .child(
                h_flex()
                    .px_3()
                    .when(cfg!(target_os = "macos"), |row| row.pt_3())
                    .gap_1()
                    .child(
                        Button::new("open-file")
                            .ghost()
                            .small()
                            .icon(IconName::File)
                            .tooltip("Open file")
                            .accessibility_label("Open file")
                            .on_click(cx.listener(|this, _, w, cx| this.pick(false, w, cx))),
                    )
                    .child(
                        Button::new("open-folder")
                            .ghost()
                            .small()
                            .icon(IconName::FolderOpen)
                            .tooltip("Open folder")
                            .accessibility_label("Open folder")
                            .on_click(cx.listener(|this, _, w, cx| this.pick(true, w, cx))),
                    ),
            )
            .child(
                h_flex()
                    .px_5()
                    .pt_6()
                    .pb_2()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("WORKSPACE"),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("refresh")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::RefreshCw)
                                    .tooltip("Refresh folder")
                                    .accessibility_label("Refresh folder")
                                    .disabled(self.root.is_none())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let paths: Vec<_> = this.expanded.iter().cloned().collect();
                                        for path in paths {
                                            this.load_folder(path, cx);
                                        }
                                    })),
                            )
                            .child(
                                Button::new("new")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Plus)
                                    .tooltip("New file")
                                    .accessibility_label("New file")
                                    .on_click(cx.listener(|this, _, w, cx| this.new_file(w, cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .px_5()
                    .pb_3()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(
                        self.root
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "No folder open".into()),
                    ),
            )
            .when(self.naming, |s| {
                s.child(
                    v_flex()
                        .px_3()
                        .pb_3()
                        .gap_2()
                        .child(Input::new(&self.filename).small())
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("create")
                                        .small()
                                        .primary()
                                        .label("Create")
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.create_file(w, cx)),
                                        ),
                                )
                                .child(
                                    Button::new("cancel")
                                        .small()
                                        .ghost()
                                        .label("Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.naming = false;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
            .child(
                v_flex()
                    .id("files")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .children(rows.into_iter().enumerate().map(|(i, (entry, depth))| {
                        let path = entry.path.clone();
                        let expanded = self.expanded.contains(&path);
                        let label = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        Button::new(("file", i))
                            .ghost()
                            .small()
                            .w_full()
                            .justify_start()
                            .pl(px(8. + depth as f32 * 14.))
                            .icon(if entry.directory {
                                if expanded {
                                    IconName::FolderOpen
                                } else {
                                    IconName::Folder
                                }
                            } else {
                                IconName::File
                            })
                            .label(label)
                            .when(active_path.as_ref() == Some(&path), |b| {
                                b.bg(theme.secondary)
                            })
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if entry.directory {
                                    if !this.expanded.remove(&path) {
                                        this.expanded.insert(path.clone());
                                        this.load_folder(path.clone(), cx);
                                    }
                                    cx.notify();
                                } else {
                                    this.open_path(path.clone(), w, cx);
                                }
                            }))
                    })),
            )
            .child(
                h_flex().px_3().py_3().child(
                    Button::new("settings")
                        .ghost()
                        .small()
                        .icon(IconName::Settings)
                        .tooltip("Settings")
                        .accessibility_label("Settings")
                        .on_click(|_, _, cx| settings::open(cx)),
                ),
            );

        let mut body = h_flex().flex_1().min_h_0().w_full();
        if let Some(i) = self.active {
            let document = &self.documents[i];
            let mut editor_pane = None;
            let mut reading_pane = None;
            if mode != Mode::Read {
                editor_pane = Some(
                    div()
                        .id(("editor-pane", document.editor.entity_id()))
                        .overflow_hidden()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .border_r_1()
                        .border_color(theme.border)
                        .child(
                            Editor::new(&document.editor)
                                .bordered(false)
                                .size_full()
                                .font_family(theme.mono_font_family.clone())
                                .text_sm(),
                        )
                        .into_any_element(),
                );
            }
            if mode != Mode::Edit {
                if let Some(reader) = &document.large_reader {
                    reading_pane = Some(
                        div()
                            .id(("preview-pane", document.editor.entity_id()))
                            .overflow_hidden()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(reader.clone())
                            .into_any_element(),
                    );
                } else {
                    let base = document
                        .path
                        .as_ref()
                        .and_then(|p| p.parent())
                        .map(PathBuf::from);
                    let link_base = base.clone();
                    reading_pane = Some(
                        div()
                            .id(("preview-pane", document.editor.entity_id()))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .px_10()
                            .py_8()
                            .child(
                                div().max_w(px(800.)).mx_auto().size_full().child(
                                    TextView::new(&document.preview)
                                        .selectable(true)
                                        .scrollable(true)
                                        .size_full()
                                        .image_source(move |uri| {
                                            let source = uri.to_string();
                                            if !source.contains(':') {
                                                if let Some(base) = &base {
                                                    return ImageSource::from(base.join(source));
                                                }
                                            }
                                            ImageSource::from(uri.clone())
                                        })
                                        .on_link_click(move |url, event, _, cx| {
                                            if event.is_right_click() {
                                                return;
                                            }
                                            if url.starts_with("https://")
                                                || url.starts_with("http://")
                                                || url.starts_with("mailto:")
                                            {
                                                cx.open_url(url);
                                            } else if !url.contains(':') && !url.starts_with('#') {
                                                if let Some(base) = &link_base {
                                                    cx.reveal_path(&base.join(url.as_ref()));
                                                }
                                            }
                                        }),
                                ),
                            )
                            .into_any_element(),
                    );
                }
            }
            // Read and Edit have independent element paths. Only Split owns
            // resizable state, so switching modes never reuses panel zero for
            // a different kind of document view or its cached draw state.
            body = match (editor_pane, reading_pane) {
                (Some(editor), Some(reader)) => body.child(
                    h_resizable(("document-split", document.editor.entity_id()))
                        .child(
                            resizable_panel()
                                .size_range(px(180.)..Pixels::MAX)
                                .child(editor),
                        )
                        .child(
                            resizable_panel()
                                .size_range(px(180.)..Pixels::MAX)
                                .child(reader),
                        ),
                ),
                (Some(editor), None) => body.child(editor),
                (None, Some(reader)) => body.child(reader),
                (None, None) => body,
            };
        } else {
            body = body.child(
                v_flex()
                    .id("empty-workspace")
                    .size_full()
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .gap_5()
                    .child(
                        v_flex()
                            .items_center()
                            .gap_3()
                            .child(brand::logo(104., cx))
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("enso"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Open a file, find a thought, or start something new."),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("start-open")
                                    .small()
                                    .primary()
                                    .icon(IconName::File)
                                    .label("Open file")
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.pick(false, w, cx)),
                                    ),
                            )
                            .child(
                                Button::new("start-folder")
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label("Open folder")
                                    .on_click(cx.listener(|this, _, w, cx| this.pick(true, w, cx))),
                            )
                            .child(
                                Button::new("start-new")
                                    .small()
                                    .ghost()
                                    .icon(IconName::Plus)
                                    .label("New note")
                                    .on_click(cx.listener(|this, _, w, cx| this.new_file(w, cx))),
                            ),
                    ),
            );
        }
        let (mac_tabs, standard_tabs) = if cfg!(target_os = "macos") {
            (Some(tabs), None)
        } else {
            (None, Some(tabs))
        };
        let document_header = h_flex()
            .h(px(if cfg!(target_os = "macos") { 44. } else { 64. }))
            .flex_1()
            .min_w_0()
            .px_6()
            .when(cfg!(target_os = "macos"), |header| header.px_2())
            .items_center()
            .justify_between()
            .when(!cfg!(target_os = "macos"), |header| {
                header.border_b_1().border_color(theme.border)
            })
            .when(!cfg!(target_os = "macos"), |header| {
                header.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(document_title),
                        )
                        .when(!cfg!(target_os = "macos"), |title| {
                            title.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(if markdown { "MARKDOWN" } else { "PLAIN TEXT" }),
                            )
                        }),
                )
            })
            .when_some(mac_tabs, |header, tabs| header.child(tabs))
            .child(
                h_flex().gap_1().items_center().flex_shrink_0().children(
                    [
                        (Mode::Read, "Read", IconName::BookOpen),
                        (Mode::Edit, "Edit", IconName::Pencil),
                        (Mode::Split, "Split", IconName::Columns2),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (value, label, icon))| {
                        Button::new(("mode", i))
                            .ghost()
                            .small()
                            .icon(icon)
                            .tooltip(label)
                            .accessibility_label(label)
                            .disabled(!markdown || self.active.is_none())
                            .when(mode == value, |b| b.bg(theme.secondary))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_mode(value, window, cx)
                            }))
                    }),
                ),
            );
        let (mac_titlebar, standard_header) = if cfg!(target_os = "macos") {
            let toggle_label = if self.sidebar_open {
                "Collapse sidebar"
            } else {
                "Expand sidebar"
            };
            let titlebar = TitleBar::new()
                .h(px(44.))
                .bg(theme.background)
                .border_color(theme.border)
                .child(
                    h_flex()
                        .w_full()
                        .h_full()
                        .items_center()
                        .gap_2()
                        .pr_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .flex_shrink_0()
                                .child(brand::logo(24., cx))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("enso"),
                                ),
                        )
                        .child(
                            Button::new("toggle-sidebar")
                                .ghost()
                                .small()
                                .icon(IconName::PanelLeft)
                                .tooltip(toggle_label)
                                .accessibility_label(toggle_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sidebar_open = !this.sidebar_open;
                                    cx.notify();
                                })),
                        )
                        .child(document_header),
                );
            (Some(titlebar.into_any_element()), None)
        } else {
            (None, Some(document_header.into_any_element()))
        };
        let stats = self
            .active
            .map(|i| self.documents[i].stats.clone())
            .unwrap_or_default();
        v_flex()
            .id("enso")
            .key_context("Enso")
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &OpenFile, w, cx| this.pick(false, w, cx)))
            .on_action(cx.listener(|this, _: &OpenFolder, w, cx| this.pick(true, w, cx)))
            .on_action(cx.listener(|this, _: &NewFile, w, cx| this.new_file(w, cx)))
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save(cx)))
            .on_action(cx.listener(|this, _: &CloseFile, _, cx| this.close_current_file(cx)))
            .on_action(cx.listener(|this, _: &Quit, w, cx| this.request_quit(w, cx)))
            .on_action(cx.listener(|this, _: &ToggleView, window, cx| {
                let next = if this.mode == Mode::Edit {
                    Mode::Read
                } else {
                    Mode::Edit
                };
                this.set_mode(next, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSplit, window, cx| {
                let next = if this.mode == Mode::Split {
                    this.previous_mode
                } else {
                    Mode::Split
                };
                this.set_mode(next, window, cx);
            }))
            .when_some(mac_titlebar, |view, titlebar| view.child(titlebar))
            .child(
                div().flex_1().min_h_0().w_full().child(
                    h_flex()
                        .id("workspace-panels")
                        .size_full()
                        .on_drag_move(cx.listener(
                            |this, event: &DragMoveEvent<SidebarResize>, _, cx| {
                                if !this.sidebar_open {
                                    return;
                                }
                                let drag = event.drag(cx);
                                let width = (drag.width + event.event.position.x - drag.pointer_x)
                                    .clamp(px(160.), px(480.));
                                if this.sidebar_width != width {
                                    this.sidebar_width = width;
                                    cx.notify();
                                }
                            },
                        ))
                        .child(
                            div()
                                .id("sidebar-viewport")
                                .relative()
                                .h_full()
                                .flex_shrink_0()
                                .w(sidebar_width * sidebar_reveal)
                                .overflow_hidden()
                                .when(sidebar_reveal <= 0.001, |sidebar| sidebar.invisible())
                                .child(div().w(sidebar_width).h_full().child(sidebar))
                                .when(self.sidebar_open && sidebar_reveal >= 0.999, |sidebar| {
                                    sidebar.child(
                                        resize_handle("sidebar-resize", Axis::Horizontal)
                                            .inside(HandleEdge::Trailing)
                                            .on_drag(
                                                SidebarResize {
                                                    width: sidebar_width,
                                                    pointer_x: px(0.),
                                                },
                                                |drag, _, window, cx| {
                                                    let value = SidebarResize {
                                                        width: drag.width,
                                                        pointer_x: window.mouse_position().x,
                                                    };
                                                    cx.new(|_| value)
                                                },
                                            ),
                                    )
                                }),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .when_some(standard_header, |view, header| view.child(header))
                                .when_some(standard_tabs, |view, tabs| {
                                    view.when(!self.documents.is_empty(), |view| view.child(tabs))
                                })
                                .child(body)
                                .child(
                                    h_flex()
                                        .h(px(20.))
                                        .flex_shrink_0()
                                        .items_center()
                                        .overflow_hidden()
                                        .px_3()
                                        .py_0()
                                        .gap_3()
                                        .justify_between()
                                        .border_t_1()
                                        .border_color(theme.border)
                                        .text_size(px(10.))
                                        .line_height(relative(1.))
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_color(if self.error {
                                                    theme.danger
                                                } else {
                                                    theme.muted_foreground
                                                })
                                                .child(if self.error {
                                                    self.status.clone()
                                                } else {
                                                    String::new()
                                                }),
                                        )
                                        .child(
                                            div().text_color(theme.muted_foreground).child(stats),
                                        ),
                                ),
                        ),
                ),
            )
    }
}

fn bind_view_keys(cx: &mut App) {
    // Bind at the input's context depth too: macOS inputs normally
    // consume Ctrl+E as end-of-line before a parent action can see it.
    for context in ["Enso", "Enso > Input"] {
        cx.bind_keys([
            KeyBinding::new("ctrl-e", ToggleView, Some(context)),
            KeyBinding::new("ctrl-w", CloseFile, Some(context)),
            KeyBinding::new("ctrl-\\", ToggleSplit, Some(context)),
        ]);
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("cmd-e", ToggleView, Some(context)),
                KeyBinding::new("cmd-w", CloseFile, Some(context)),
                KeyBinding::new("cmd-\\", ToggleSplit, Some(context)),
            ]);
        }
    }
}

fn main() {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            app_icon::init();
            settings::init(cx);
            cx.on_action(|_: &OpenSettings, cx| settings::open(cx));
            cx.bind_keys([
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("ctrl-,", OpenSettings, None),
            ]);
            macro_rules! shortcut {
                ($key:literal, $action:expr) => {
                    KeyBinding::new(
                        if cfg!(target_os = "macos") {
                            concat!("cmd-", $key)
                        } else {
                            concat!("ctrl-", $key)
                        },
                        $action,
                        Some("Enso"),
                    )
                };
            }
            cx.bind_keys([
                shortcut!("o", OpenFile),
                shortcut!("shift-o", OpenFolder),
                shortcut!("n", NewFile),
                shortcut!("s", Save),
                shortcut!("e", ToggleView),
                shortcut!("q", Quit),
            ]);
            bind_view_keys(cx);
            cx.set_menus(vec![
                Menu {
                    name: "Enso".into(),
                    items: vec![
                        MenuItem::action("Settings…", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("Quit Enso", Quit),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "File".into(),
                    items: vec![
                        MenuItem::action("Open File…", OpenFile),
                        MenuItem::action("Open Folder…", OpenFolder),
                        MenuItem::action("New File", NewFile),
                        MenuItem::action("Close File", CloseFile),
                        #[cfg(not(target_os = "macos"))]
                        MenuItem::action("Settings…", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("Save", Save),
                    ],
                    disabled: false,
                },
            ]);
            let (main_window, main_view) = gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(if cfg!(target_os = "macos") {
                        TitlebarOptions {
                            traffic_light_position: Some(point(px(12.), px(14.))),
                            ..TitleBar::title_bar_options()
                        }
                    } else {
                        TitlebarOptions {
                            title: Some("Enso".into()),
                            ..Default::default()
                        }
                    }),
                    app_owns_titlebar_drag: cfg!(target_os = "macos"),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1180.), px(780.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(800.), px(500.))),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    cx.new(|cx| {
                        let mut app = Enso::new(window, cx);
                        if let Some(path) = initial {
                            app.open_path(std::fs::canonicalize(&path).unwrap_or(path), window, cx);
                        } else {
                            app.open_first_launch_sample(window, cx);
                        }
                        app
                    })
                },
            )
            .expect("Unable to open Enso window");
            let main_view = main_view.downgrade();
            cx.on_action(move |_: &Quit, cx| {
                if main_window
                    .update(cx, |_, window, cx| {
                        let _ = main_view.update(cx, |view, cx| view.request_quit(window, cx));
                    })
                    .is_err()
                {
                    cx.quit();
                }
            });
            cx.activate(true);
        });
}

#[cfg(all(test, feature = "ui-tests"))]
mod transition_tests {
    use super::{Enso, Mode, bind_view_keys, reader};
    use ::core::prelude::v1::test;
    use gpui_kit::Focusable;
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
    };
    use std::path::PathBuf;

    fn draw(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn repeated_shortcuts_replace_editor_with_reader(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        let source = if let Ok(path) = std::env::var("ENSO_BENCH_FILE") {
            crate::files::read_text(std::path::Path::new(&path)).unwrap()
        } else {
            "# A heading\n\nSome **Markdown** text.\n\n".repeat(32_000)
        };
        let expected = source.clone();
        let sections = reader::LargeReader::index(&source);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Default::default(),
                        size: size(px(1000.), px(700.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view = Enso::new(window, cx);
                        view.add_document(
                            Some(PathBuf::from("large.md")),
                            source,
                            Some(sections),
                            window,
                            cx,
                        );
                        view
                    })
                },
            )
            .unwrap()
        });
        draw(cx, handle);
        for iteration in 0..4 {
            let started = std::time::Instant::now();
            cx.simulate_keystrokes(handle, "ctrl-e");
            draw(cx, handle);
            if iteration == 0 {
                eprintln!("First Edit transition: {:?}", started.elapsed());
            }
            cx.update_window(handle, |_, window, cx| {
                let state = view.read(cx);
                assert!(state.mode == Mode::Edit);
                assert!(
                    state.documents[0]
                        .editor
                        .focus_handle(cx)
                        .is_focused(window)
                );
                assert_eq!(
                    state.documents[0].editor.read(cx).value(),
                    expected.as_str()
                );
            })
            .unwrap();
            // Match the reported case: leave Edit while scrolled deep into
            // a large file, rather than only testing the first screenful.
            cx.update_window(handle, |_, window, cx| {
                let editor = view.read(cx).documents[0].editor.clone();
                editor.update(cx, |editor, cx| {
                    editor.set_cursor_position(
                        gpui_kit::component::input::Position::new(124_000, 0),
                        window,
                        cx,
                    )
                });
            })
            .unwrap();
            draw(cx, handle);
            cx.simulate_keystrokes(handle, "ctrl-e");
            draw(cx, handle);
            cx.update_window(handle, |_, window, cx| {
                let state = view.read(cx);
                assert!(state.mode == Mode::Read);
                let doc = &state.documents[0];
                let cursor = doc.editor.read(cx).cursor();
                let range = doc
                    .large_reader
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .visible_source_range()
                    .unwrap();
                assert!(
                    range.start <= cursor && cursor <= range.end,
                    "cursor {cursor} outside visible source {range:?}"
                );
                assert!(
                    !state.documents[0]
                        .editor
                        .focus_handle(cx)
                        .is_focused(window)
                );
                assert!(!state.documents[0].dirty);
            })
            .unwrap();
        }
        cx.simulate_keystrokes(handle, "ctrl-\\");
        draw(cx, handle);
        cx.update(|cx| {
            let state = view.read(cx);
            assert!(state.mode == Mode::Split);
            let doc = &state.documents[0];
            let cursor = doc.editor.read(cx).cursor();
            let range = doc
                .large_reader
                .as_ref()
                .unwrap()
                .read(cx)
                .visible_source_range()
                .unwrap();
            assert!(range.start <= cursor && cursor <= range.end);
        });
        // Move the preview elsewhere, then click in the source editor. The
        // preview must follow the click without toggling modes again.
        let before_cursor = cx
            .update_window(handle, |_, _, cx| {
                let state = view.read(cx);
                let doc = &state.documents[0];
                let cursor = doc.editor.read(cx).cursor();
                let reader = doc.large_reader.as_ref().unwrap().clone();
                reader.update(cx, |reader, cx| {
                    reader.reveal_source(expected.len() - 1, cx)
                });
                cursor
            })
            .unwrap();
        draw(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            use gpui_kit::InputEvent as _;
            let bounds = view.read(cx).documents[0].editor.read(cx).input_bounds();
            let position = gpui_kit::point(bounds.left() + px(100.), bounds.top() + px(100.));
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        draw(cx, handle);
        cx.update(|cx| {
            let state = view.read(cx);
            let doc = &state.documents[0];
            let cursor = doc.editor.read(cx).cursor();
            assert_ne!(cursor, before_cursor);
            let range = doc
                .large_reader
                .as_ref()
                .unwrap()
                .read(cx)
                .visible_source_range()
                .unwrap();
            assert!(
                range.start <= cursor && cursor <= range.end,
                "click at {cursor} outside preview {range:?}"
            );
        });
        cx.simulate_keystrokes(handle, "ctrl-\\");
        draw(cx, handle);
        cx.update(|cx| assert!(view.read(cx).mode == Mode::Read));
    }
    #[gpui_kit::test]
    fn closing_tabs_flushes_saves_and_keeps_async_writes_on_the_right_file(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        let directory = std::env::temp_dir().join(format!("enso-close-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let paths: Vec<_> = ["a.md", "b.md", "c.md"]
            .into_iter()
            .map(|name| directory.join(name))
            .collect();
        for path in &paths {
            std::fs::write(path, "original").unwrap();
        }
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Default::default(),
                        size: size(px(1000.), px(700.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view = Enso::new(window, cx);
                        for path in &paths {
                            view.add_document(
                                Some(path.clone()),
                                "original".into(),
                                None,
                                window,
                                cx,
                            );
                        }
                        view
                    })
                },
            )
            .unwrap()
        });
        draw(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                for index in [1, 2] {
                    view.documents[index].editor.update(cx, |state, cx| {
                        state.set_value(format!("edited-{index}"), window, cx)
                    });
                    view.documents[index].dirty = true;
                    view.documents[index].revision += 1;
                }
                view.schedule_save(1, cx);
                view.start_save(2, cx);
                // Removing an earlier tab shifts both save targets' vector indices.
                view.close_document(0, cx);
                view.close_current_file(cx);
            })
        })
        .unwrap();
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        draw(cx, handle);
        assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "original");
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "edited-1");
        assert_eq!(std::fs::read_to_string(&paths[2]).unwrap(), "edited-2");
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.documents.len(), 1);
            assert_eq!(view.documents[0].path.as_ref(), Some(&paths[1]));
            assert_eq!(view.active, Some(0));
        });
        // External conflicts keep the file open, including a Ctrl+W close.
        std::fs::write(&paths[1], "external").unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.documents[0]
                    .editor
                    .update(cx, |state, cx| state.set_value("my edit", window, cx));
                view.documents[0].dirty = true;
                view.documents[0].revision += 1;
            })
        })
        .unwrap();
        cx.simulate_keystrokes(handle, "ctrl-w");
        draw(cx, handle);
        cx.update(|cx| {
            assert_eq!(view.read(cx).documents.len(), 1);
            assert!(view.read(cx).error);
        });
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "external");
        // After resolving the conflict, the final shortcut closes the tab,
        // leaving the application window and an empty document list alive.
        std::fs::write(&paths[1], "edited-1").unwrap();
        cx.simulate_keystrokes(handle, "ctrl-w");
        draw(cx, handle);
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "my edit");
        cx.update(|cx| {
            assert!(view.read(cx).documents.is_empty());
            assert!(view.read(cx).active.is_none());
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
}
