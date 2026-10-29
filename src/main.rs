mod app_icon;
mod brand;
mod files;
mod markdown_edit;
mod onboarding;
mod reader;
mod settings;
mod stats;
mod workspaces;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Transition, transition};
#[cfg(all(test, feature = "ui-tests"))]
extern crate gpui_kit as gpui;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Editor, EditorState, Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    popover::Popover,
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
        NewFolder,
        RenameSelected,
        DeleteSelected,
        Quit
    ]
);

// Markdown formatting in the document editor; see `bind_markdown_keys`.
actions!(
    markdown,
    [
        Bold,
        Italic,
        Strikethrough,
        InlineCode,
        InsertLink,
        Heading1,
        Heading2,
        Heading3,
        Heading4,
        Heading5,
        Heading6,
        BodyText,
        BulletList,
        NumberedList,
        TaskList,
        ToggleTaskDone,
        Quote,
        CodeBlock,
        ContinueList,
        IndentListItem,
        OutdentListItem
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
    /// Whether `preview` holds this document's rendered Markdown. Only the few
    /// most recently viewed documents keep it; see `Enso::warm_documents`.
    preview_loaded: bool,
    /// When this document was last the active one (`Enso::view_clock`).
    last_viewed: u64,
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
struct SidebarResize;

/// An image pasted into a note: raw data (a screenshot) or a file copied in a
/// file manager.
enum PastedImage {
    Data(gpui_kit::Image),
    File(PathBuf),
}

/// An entry being named in the sidebar before it exists on disk.
struct NewEntry {
    dir: PathBuf,
    folder: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseTarget {
    Window,
    App,
}

/// Every open workspace window, so a workspace is only ever open in one.
#[derive(Default)]
struct OpenWindows(Vec<(AnyWindowHandle, WeakEntity<Enso>)>);
impl Global for OpenWindows {}

fn open_views(cx: &App) -> Vec<(AnyWindowHandle, Entity<Enso>)> {
    cx.try_global::<OpenWindows>()
        .map(|w| {
            w.0.iter()
                .filter_map(|(handle, view)| Some((*handle, view.upgrade()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// The window already showing the workspace at `path`, if any. `except` is
/// the view asking: it is mid-update, so reading it would panic.
fn window_for(path: &PathBuf, except: EntityId, cx: &App) -> Option<AnyWindowHandle> {
    open_views(cx)
        .into_iter()
        .filter(|(_, view)| view.entity_id() != except)
        .find(|(_, view)| view.read(cx).root.as_ref() == Some(path))
        .map(|(handle, _)| handle)
}

fn activate(handle: AnyWindowHandle, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| window.activate_window());
}

/// Quit once no window has unsaved or in-flight edits left.
fn quit_when_saved(cx: &mut App) {
    let pending = open_views(cx).into_iter().any(|(_, view)| {
        let view = view.read(cx);
        view.documents.iter().any(|d| d.dirty || d.saving)
    });
    if !pending {
        cx.quit();
    }
}

fn quit_all(cx: &mut App) {
    let views = open_views(cx);
    if views.is_empty() {
        cx.quit();
        return;
    }
    for (handle, view) in views {
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |view, cx| view.request_close(CloseTarget::App, window, cx))
        });
    }
}

/// One visible line of the file tree.
struct TreeRow {
    entry: files::Entry,
    /// For each ancestor below the root, whether it has siblings after it, so
    /// its thread keeps running down past this row.
    threads: Vec<bool>,
    last: bool,
    /// The placeholder for a file or folder being named, not yet on disk.
    pending: bool,
}

const TREE_INDENT: f32 = 16.;
/// Height of the macOS title bar strip that sits over the top of the window.
const TITLEBAR_HEIGHT: f32 = 38.;
/// Left inset GPUI Kit's title bar keeps: room for the macOS traffic lights,
/// a margin on Windows (its window controls are on the right).
const TRAFFIC_LIGHTS_WIDTH: f32 = if cfg!(target_os = "macos") { 80. } else { 12. };
/// macOS and Windows draw Enso's own title bar (tabs and view buttons in it,
/// the sidebar running up beside the window controls); Linux keeps the
/// window manager's.
const CUSTOM_TITLEBAR: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// The system backdrop behind the window: a blurred tint of the desktop on
/// macOS, Mica on Windows 11, nothing elsewhere. Only the sidebar lets it show.
fn window_backdrop() -> WindowBackgroundAppearance {
    if cfg!(target_os = "macos") {
        WindowBackgroundAppearance::Blurred
    } else if cfg!(target_os = "windows") {
        WindowBackgroundAppearance::MicaBackdrop
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

fn translucent() -> bool {
    window_backdrop() != WindowBackgroundAppearance::Opaque
}

/// The content column's background. Over a system backdrop it lets a little of
/// the tint through, so the sidebar and content read as one surface rather
/// than a hard light/dark split.
/// Whether the system backdrop actually shows: there is one, and it is as
/// light or dark as Enso's theme. macOS's follows the appearance Enso sets;
/// Windows' Mica follows the system theme, so a Light Enso on a dark Windows
/// (or the reverse) would show a dark backdrop under light content. Then
/// everything is painted solid instead.
fn backdrop_visible(window: &Window, cx: &App) -> bool {
    let backdrop_dark = matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    translucent() && backdrop_dark == cx.theme().mode.is_dark()
}

fn content_background(window: &Window, cx: &App) -> Hsla {
    let theme = cx.theme();
    if backdrop_visible(window, cx) {
        theme
            .background
            .opacity(if theme.mode.is_dark() { 0.72 } else { 0.78 })
    } else {
        theme.background
    }
}
const TREE_ROW_HEIGHT: f32 = 26.;
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
    /// Whether the last error is still on the status bar; it hides itself.
    error_shown: bool,
    _error_task: Option<Task<()>>,
    naming: Option<NewEntry>,
    filename: Entity<InputState>,
    /// The tree row last clicked, file or folder; new entries go beside it.
    selected: Option<PathBuf>,
    switcher_open: bool,
    switcher_hovered: bool,
    window_handle: AnyWindowHandle,
    renaming: Option<PathBuf>,
    rename_input: Entity<InputState>,
    closing: Option<CloseTarget>,
    sidebar_open: bool,
    sidebar_width: Pixels,
    focus_after_close: bool,
    hovered_tab: Option<EntityId>,
    /// The tab strip's scroll position, and the tab last scrolled into view.
    tab_scroll: ScrollHandle,
    scrolled_to_tab: Option<EntityId>,
    /// Counts document switches, to tell which documents were viewed last.
    view_clock: u64,
    warmed_doc: Option<EntityId>,
    _subscriptions: Vec<Subscription>,
}

impl Enso {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("Enso");
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let filename = cx.new(|cx| InputState::new(window, cx));
        let subscription = cx.subscribe_in(&filename, window, |this, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => this.create_entry(window, cx),
                // Clicking away abandons the new entry rather than creating
                // one by accident.
                InputEvent::Blur => this.cancel_naming(window, cx),
                _ => {}
            }
        });
        let rename_input = cx.new(|cx| InputState::new(window, cx));
        let rename_subscription =
            cx.subscribe_in(&rename_input, window, |this, _, event, window, cx| {
                // Enter commits, and so does clicking away, as in Finder.
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.commit_rename(window, cx);
                }
            });
        let weak = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.default_global::<OpenWindows>()
            .0
            .push((window_handle, weak.clone()));
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.documents.iter().any(|d| d.dirty(cx)) {
                    this.request_close(CloseTarget::Window, window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        let history = cx.observe_global::<workspaces::Workspaces>(|_, cx| cx.notify());
        let preferences =
            cx.observe_global_in::<settings::Preferences>(window, |this, window, cx| {
                let on = settings::line_numbers(cx);
                for doc in &this.documents {
                    doc.editor
                        .update(cx, |editor, cx| editor.set_line_number(on, window, cx));
                }
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
            error_shown: false,
            _error_task: None,
            naming: None,
            filename,
            selected: None,
            switcher_open: false,
            switcher_hovered: false,
            window_handle,
            renaming: None,
            rename_input,
            closing: None,
            sidebar_open: true,
            sidebar_width: px(248.),
            focus_after_close: false,
            hovered_tab: None,
            tab_scroll: ScrollHandle::new(),
            scrolled_to_tab: None,
            view_clock: 0,
            warmed_doc: None,
            _subscriptions: vec![
                subscription,
                rename_subscription,
                history,
                preferences,
                settings::watch_system(window),
            ],
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

    /// Keep the rendered preview only for the documents viewed most recently:
    /// parse the active one's if it has none, and drop the rest's. Background
    /// tabs then cost a file's text, not a parsed and laid-out document.
    fn warm_documents(&mut self, cx: &mut Context<Self>) {
        const KEEP: usize = 3;
        let Some(active) = self.active else {
            return;
        };
        self.view_clock += 1;
        self.documents[active].last_viewed = self.view_clock;
        let doc = &mut self.documents[active];
        if doc.markdown && doc.large_reader.is_none() && !doc.preview_loaded {
            let text = doc.editor.read(cx).value();
            // Large files read through `large_reader`; their editor may not be
            // filled yet, and the preview stays empty either way.
            if text.len() < reader::LARGE_DOCUMENT_BYTES {
                doc.preview.update(cx, |preview, cx| preview.set_text(&text, cx));
                doc.preview_loaded = true;
            }
        }
        let mut recent: Vec<_> = (0..self.documents.len()).collect();
        recent.sort_by_key(|&i| std::cmp::Reverse(self.documents[i].last_viewed));
        for &i in recent.iter().skip(KEEP) {
            let doc = &mut self.documents[i];
            if doc.preview_loaded {
                doc.preview.update(cx, |preview, cx| preview.set_text("", cx));
                doc.preview_loaded = false;
            }
        }
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
        self.error_shown = error;
        // Errors fade from the bar after a while; `error` itself stays set,
        // since quitting relies on it to know auto-save failed.
        self._error_task = error.then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(5))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.error_shown = false;
                    cx.notify();
                });
            })
        });
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
                // No fold column: it widens the line-number gutter for a
                // feature prose rarely needs.
                .folding(false)
                .auto_close(!large)
                .line_number(settings::line_numbers(cx))
                .default_value(if large && markdown {
                    String::new()
                } else {
                    source.as_ref().clone()
                })
        });
        let wrap = source.len() < reader::NO_WRAP_BYTES;
        editor.update(cx, |editor, cx| editor.set_soft_wrap(wrap, window, cx));
        // Parsed when the document is first shown, not on open: opening many
        // files at once only reads them (see `warm_documents`).
        let preview = cx.new(|cx| TextViewState::markdown("", cx));
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
                            && doc.preview_loaded
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
            preview_loaded: false,
            last_viewed: 0,
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
            self.switch_workspace(path, false, window, cx);
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
        self.selected = None;
        self.naming = None;
        self.renaming = None;
        self.expanded.insert(path.clone());
        workspaces::record(&path, cx);
        let title = path
            .file_name()
            .map(|n| format!("{} — Enso", n.to_string_lossy()))
            .unwrap_or_else(|| "Enso".into());
        let handle = self.window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.set_window_title(&title));
        });
        self.load_folder(path, cx);
    }

    /// Show the workspace at `path`: in the window that already has it, in a
    /// new window, or in place of this window's workspace.
    fn switch_workspace(
        &mut self,
        path: PathBuf,
        new_window: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switcher_open = false;
        cx.notify();
        if !path.is_dir() {
            self.message(
                format!("“{}” is no longer available.", path.display()),
                true,
                cx,
            );
            return;
        }
        if self.root.as_ref() == Some(&path) {
            return;
        }
        if let Some(handle) = window_for(&path, cx.entity_id(), cx) {
            cx.defer(move |cx| activate(handle, cx));
            return;
        }
        if new_window {
            cx.defer(move |cx| open_workspace_window(Some(path), cx));
            return;
        }
        // Tabs belong to the workspace they were opened from. Closing flushes
        // pending edits first, as closing a tab by hand does.
        for index in (0..self.documents.len()).rev() {
            self.close_document(index, cx);
        }
        self.open_folder(path, cx);
        self.focus.focus(window, cx);
    }

    /// Where a new file or folder goes: into the selected folder, beside the
    /// selected file, or at the top of the workspace.
    fn target_dir(&self) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let dir = match &self.selected {
            Some(path) if path.is_dir() => path.clone(),
            Some(path) => path.parent()?.to_path_buf(),
            None => root.clone(),
        };
        Some(if dir.starts_with(root) { dir } else { root.clone() })
    }

    fn start_naming(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.target_dir() else {
            return;
        };
        if dir != *self.root.as_ref().unwrap() && self.expanded.insert(dir.clone()) {
            self.load_folder(dir.clone(), cx);
        }
        let (value, stem) = if folder {
            ("New Folder", "New Folder".len())
        } else {
            ("Untitled.md", "Untitled".len())
        };
        self.naming = Some(NewEntry { dir, folder });
        self.filename.update(cx, |s, cx| {
            s.set_value(value, window, cx);
            s.set_selected_range(0..stem, cx);
        });
        self.filename.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn cancel_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.naming.take().is_some() {
            if self.filename.focus_handle(cx).is_focused(window) {
                self.focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.root.is_some() {
            self.start_naming(true, window, cx);
        } else {
            self.message("Open a folder to add folders to it.", true, cx);
        }
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
            self.start_naming(false, window, cx);
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

    fn create_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(NewEntry { dir, folder }) = self.naming.take() else {
            return;
        };
        let name = self.filename.read(cx).value().to_string();
        let created = if folder {
            files::create_folder(&dir, &name)
        } else {
            files::create_file(&dir, &name)
        };
        match created {
            Ok(path) => {
                self.load_folder(dir, cx);
                self.selected = Some(path.clone());
                if folder {
                    self.focus.focus(window, cx);
                } else {
                    self.add_document(Some(path), String::new(), None, window, cx);
                    self.mode = Mode::Edit;
                    if let Some(i) = self.active {
                        self.documents[i].editor.focus_handle(cx).focus(window, cx);
                    }
                }
                cx.notify();
            }
            Err(error) => {
                // Keep the field open so the name can be corrected.
                self.naming = Some(NewEntry { dir, folder });
                self.message(error.to_string(), true, cx);
            }
        }
    }

    /// Save pasted images under the workspace's hidden `.assets/images` and
    /// insert an `<img>` for each at the cursor, one per line. A file outside the workspace keeps
    /// its images beside it instead, in its own folder's `.assets/images`.
    fn paste_images(
        &mut self,
        images: Vec<PastedImage>,
        editor: Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc_path) = self
            .documents
            .iter()
            .find(|doc| doc.editor.entity_id() == editor.entity_id())
            .and_then(|doc| doc.path.clone())
        else {
            self.message("Save this note before pasting images into it.", true, cx);
            return;
        };
        let Some(doc_dir) = doc_path.parent().map(PathBuf::from) else {
            return;
        };
        let root = self
            .root
            .clone()
            .filter(|root| doc_dir.starts_with(root))
            .unwrap_or_else(|| doc_dir.clone());
        let mut links = Vec::new();
        for image in images {
            let saved = match &image {
                PastedImage::Data(image) => {
                    files::save_image(&root, None, &image.bytes, image.format.extension())
                }
                PastedImage::File(path) => files::import_image(&root, path),
            };
            match saved {
                Ok(saved) => {
                    let alt = saved
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "image".into());
                    // An <img> tag rather than ![](): Markdown images cannot be
                    // sized, and this one starts at the reading column's width.
                    let attr = |value: &str| {
                        value
                            .replace('&', "&amp;")
                            .replace('"', "&quot;")
                            .replace('<', "&lt;")
                    };
                    let link = files::relative_link(&doc_dir, &saved);
                    links.push(format!(
                        "<img src=\"{}\" alt=\"{}\" width=\"100%\" />",
                        attr(&link),
                        attr(&alt)
                    ));
                }
                Err(error) => {
                    self.message(format!("Could not save the image: {error}"), true, cx)
                }
            }
        }
        if !links.is_empty() {
            editor.update(cx, |editor, cx| editor.replace(links.join("\n"), window, cx));
        }
    }

    /// The tree row last clicked, if it is still inside the open workspace.
    fn selected_entry(&self) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        self.selected
            .clone()
            .filter(|path| path != root && path.starts_with(root) && path.exists())
    }

    /// Move a file or folder to the Trash (Recycle Bin on Windows), without
    /// asking: it can be restored from there. Only an item that can't be
    /// recycled is ever deleted outright, and then the system asks first
    /// (Windows) or the move fails and nothing is deleted (macOS, Linux).
    fn delete_entry(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // On the main thread: Windows' "delete permanently?" dialog needs it.
        match files::move_to_trash(&path) {
            Ok(()) => self.forget_entry(&path, cx),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                self.message(format!("Kept “{name}”: {error}"), false, cx)
            }
            Err(error) => self.message(format!("Could not delete “{name}”: {error}"), true, cx),
        }
    }

    /// Drop everything that pointed at a file or folder that is gone: its
    /// tabs (their edits went with it), tree state and selection.
    fn forget_entry(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        for index in (0..self.documents.len()).rev() {
            if self.documents[index]
                .path
                .as_ref()
                .is_some_and(|p| p.starts_with(path))
            {
                self.remove_document(index, cx);
            }
        }
        self.folders.retain(|p, _| !p.starts_with(path));
        self.expanded.retain(|p| !p.starts_with(path));
        if self.selected.as_ref().is_some_and(|p| p.starts_with(path)) {
            self.selected = None;
        }
        if self.renaming.as_ref().is_some_and(|p| p.starts_with(path)) {
            self.renaming = None;
        }
        if self.naming.as_ref().is_some_and(|n| n.dir.starts_with(path)) {
            self.naming = None;
        }
        if let Some(parent) = path.parent() {
            self.load_folder(parent.to_path_buf(), cx);
        }
        cx.notify();
    }

    fn start_rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // Select the name without its extension, as Finder does.
        let stem = match name.rfind('.') {
            Some(dot) if dot > 0 => dot,
            _ => name.len(),
        };
        self.renaming = Some(path);
        self.rename_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.set_selected_range(0..stem, cx);
        });
        self.rename_input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.take().is_some() {
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.renaming.take() else {
            return;
        };
        let name = self.rename_input.read(cx).value().to_string();
        match files::rename(&old, &name) {
            Ok(new) if new != old => {
                let moved = |path: &PathBuf| {
                    path.strip_prefix(&old)
                        .ok()
                        .map(|rest| if rest.as_os_str().is_empty() { new.clone() } else { new.join(rest) })
                };
                if let Some(path) = self.selected.as_ref().and_then(|p| moved(p)) {
                    self.selected = Some(path);
                }
                for doc in &mut self.documents {
                    if let Some(path) = doc.path.as_ref().and_then(|p| moved(p)) {
                        doc.markdown = files::is_markdown(&path);
                        doc.path = Some(path);
                    }
                }
                self.folders.retain(|path, _| !path.starts_with(&old));
                self.expanded = self
                    .expanded
                    .drain()
                    .map(|path| moved(&path).unwrap_or(path))
                    .collect();
                let reload: Vec<_> = old
                    .parent()
                    .map(PathBuf::from)
                    .into_iter()
                    .chain(self.expanded.iter().filter(|p| p.starts_with(&new)).cloned())
                    .collect();
                for path in reload {
                    self.load_folder(path, cx);
                }
            }
            Ok(_) => {}
            Err(error) => self.message(error.to_string(), true, cx),
        }
        self.focus.focus(window, cx);
        cx.notify();
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
                        if let Some(target) = this.closing
                            && this.documents.iter().all(|d| !d.dirty && !d.saving)
                        {
                            this.finish_close(target, cx);
                        }
                    }
                    Err(error) => {
                        doc.close_after_save = false;
                        this.closing = None;
                        this.message(format!("Auto-save failed: {error}"), true, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Close this window, or take part in quitting the app: flush every
    /// pending edit first, and ask only when auto-save has already failed.
    fn request_close(&mut self, target: CloseTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        if !self.documents.iter().any(|d| d.dirty(cx)) {
            self.finish_close(target, cx);
            return;
        }
        if !self.error {
            self.closing = Some(target);
            for index in 0..self.documents.len() {
                self.start_save(index, cx);
            }
            return;
        }
        self.closing = Some(target);
        let (detail, discard) = match target {
            CloseTarget::Window => (
                "Automatic saving failed. Keep editing to preserve your changes, or discard them and close the window.",
                "Discard and close",
            ),
            CloseTarget::App => (
                "Automatic saving failed. Keep editing to preserve your changes, or discard and quit.",
                "Discard and quit",
            ),
        };
        let receiver = window.prompt(
            PromptLevel::Warning,
            "You have unsaved changes",
            Some(detail),
            &["Keep editing", discard],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = receiver.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.closing = None;
                if answer == Ok(1) {
                    match target {
                        CloseTarget::Window => this.remove_window(cx),
                        CloseTarget::App => cx.quit(),
                    }
                }
            });
        })
        .detach();
    }

    fn finish_close(&mut self, target: CloseTarget, cx: &mut Context<Self>) {
        self.closing = None;
        match target {
            CloseTarget::Window => self.remove_window(cx),
            CloseTarget::App => cx.defer(quit_when_saved),
        }
    }

    fn remove_window(&mut self, cx: &mut Context<Self>) {
        let handle = self.window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    }

    fn tree_rows(&self, path: &PathBuf, threads: &mut Vec<bool>, rows: &mut Vec<TreeRow>) {
        let entries = self.folders.get(path).map(Vec::as_slice).unwrap_or_default();
        // A new entry is named in place, at the top of the folder it goes in.
        if let Some(NewEntry { dir, folder }) = &self.naming
            && dir == path
        {
            rows.push(TreeRow {
                entry: files::Entry {
                    path: dir.join(".enso-new-entry"),
                    directory: *folder,
                },
                threads: threads.clone(),
                last: entries.is_empty(),
                pending: true,
            });
        }
        for (i, entry) in entries.iter().enumerate() {
            let last = i + 1 == entries.len();
            rows.push(TreeRow {
                entry: entry.clone(),
                threads: threads.clone(),
                last,
                pending: false,
            });
            if entry.directory && self.expanded.contains(&entry.path) {
                threads.push(!last);
                self.tree_rows(&entry.path, threads, rows);
                threads.pop();
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
            self.tree_rows(root, &mut Vec::new(), &mut rows);
        }
        let theme = cx.theme().clone();
        let backdrop = backdrop_visible(window, cx);
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
        let markdown = self.active.is_none_or(|i| self.documents[i].markdown);
        let mode = if markdown { self.mode } else { Mode::Edit };
        let active_doc = self.active.map(|i| self.documents[i].editor.entity_id());
        if active_doc != self.warmed_doc {
            self.warmed_doc = active_doc;
            self.warm_documents(cx);
        }
        // Bring a newly active tab into view, once, so scrolling away stays put.
        // From the last frame's layout (tab bounds there exclude the scroll
        // offset); a tab not laid out yet is revealed on the next frame.
        let active_tab = self.active.map(|i| self.documents[i].editor.entity_id());
        if active_tab != self.scrolled_to_tab {
            match self.active.and_then(|i| self.tab_scroll.bounds_for_item(i)) {
                Some(tab) => {
                    self.scrolled_to_tab = active_tab;
                    let view = self.tab_scroll.bounds();
                    let mut offset = self.tab_scroll.offset();
                    if tab.left() + offset.x < view.left() {
                        offset.x = view.left() - tab.left();
                    } else if tab.right() + offset.x > view.right() {
                        offset.x = view.right() - tab.right();
                    }
                    offset.x = offset.x.clamp(-self.tab_scroll.max_offset().x, px(0.));
                    self.tab_scroll.set_offset(offset);
                }
                None if active_tab.is_some() => window.request_animation_frame(),
                None => self.scrolled_to_tab = None,
            }
        }
        // Scrolls sideways (wheel or trackpad) once tabs overflow; no scrollbar.
        let mut tabs = h_flex()
            .id("tabs")
            .track_scroll(&self.tab_scroll)
            .gap_1()
            .h_full()
            .items_center()
            .flex_1()
            .min_w_0()
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

            let active = self.active == Some(i);
            // One pill: name and close button share the hover and active fill.
            // The wrapper keeps a press on a tab from starting a title-bar
            // window drag (after the tab's own handlers have seen it).
            tabs = tabs.child(div().flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation()
            }).child(
                h_flex()
                    .id(("file-tab", editor_id))
                    .flex_shrink_0()
                    .h(px(28.))
                    .pl_2p5()
                    .pr_1()
                    .gap_0p5()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
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
                    .when(active, |tab| tab.bg(theme.secondary))
                    .when(!active, |tab| {
                        tab.text_color(theme.muted_foreground)
                            .hover(|tab| tab.bg(theme.secondary.opacity(0.6)))
                    })
                    .child(format!(
                        "{}{}",
                        document.title(),
                        if document.dirty(cx) { " •" } else { "" }
                    ))
                    // Built only while shown: a hidden button per tab made every
                    // frame cost more with each open file.
                    .when(reveal > 0.001, |tab| tab.child(
                        h_flex()
                            .w(px(20. * reveal))
                            .h(px(20.))
                            .flex_shrink_0()
                            .overflow_hidden()
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
                                        // Not also a click on the tab around it.
                                        cx.stop_propagation();
                                        if let Some(index) = this
                                            .documents
                                            .iter()
                                            .position(|doc| doc.editor.entity_id() == editor_id)
                                        {
                                            this.close_document(index, cx);
                                        }
                                    })),
                            ),
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = this
                            .documents
                            .iter()
                            .position(|doc| doc.editor.entity_id() == editor_id);
                        this.selected = None;
                        cx.notify();
                    })),
            ));
        }
        let highlighted = self.selected.clone().or_else(|| active_path.clone());
        let switcher = {
            let weak = cx.entity().downgrade();
            let recent = workspaces::recent(cx);
            let current_root = self.root.clone();
            let name = self
                .root
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "No workspace".into());
            Popover::new("workspace-switcher")
                .open(self.switcher_open)
                .on_open_change({
                    let weak = weak.clone();
                    move |open, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.switcher_open = *open;
                            cx.notify();
                        });
                    }
                })
                .trigger(
                    Button::new("workspace")
                        .ghost()
                        .small()
                        // Icon, name and caret stay packed to the left; the caret
                        // is always laid out and only fades in, so hovering or
                        // switching never shifts the name.
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap_1p5()
                                .items_center()
                                .child(
                                    Icon::new(if self.root.is_some() {
                                        IconName::FolderOpen
                                    } else {
                                        IconName::Folder
                                    })
                                    .size_4()
                                    .flex_shrink_0(),
                                )
                                .child(div().min_w_0().truncate().child(name))
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .size_3p5()
                                        .flex_shrink_0()
                                        .text_color(theme.muted_foreground)
                                        .when(!(self.switcher_hovered || self.switcher_open), |icon| {
                                            icon.opacity(0.)
                                        }),
                                ),
                        )
                        .accessibility_label("Switch workspace")
                        .on_hover({
                            let weak = weak.clone();
                            move |hovered, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if this.switcher_hovered != *hovered {
                                        this.switcher_hovered = *hovered;
                                        cx.notify();
                                    }
                                });
                            }
                        }),
                )
                .content(move |_, _, cx| {
                    let theme = cx.theme().clone();
                    let muted = theme.muted_foreground;
                    let item = |id: ElementId| {
                        h_flex()
                            .id(id)
                            .h(px(28.))
                            .px_2()
                            .gap_1p5()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            .hover(|row| row.bg(theme.secondary.opacity(0.6)))
                    };
                    v_flex()
                        .w(px(240.))
                        .p_1()
                        .children(recent.iter().enumerate().map(|(i, recent)| {
                            let path = recent.path.clone();
                            let current = current_root.as_ref() == Some(&recent.path);
                            let exists = recent.exists();
                            let weak = weak.clone();
                            item(("workspace", i).into())
                                .when(!exists, |row| row.opacity(0.55))
                                .child(
                                    Icon::new(if current {
                                        IconName::Check
                                    } else {
                                        IconName::Folder
                                    })
                                    .size_3p5()
                                    .flex_shrink_0()
                                    .text_color(muted),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .when(current, |d| d.font_weight(FontWeight::MEDIUM))
                                        .child(recent.name()),
                                )
                                .when(!exists, |row| {
                                    row.child(div().text_xs().text_color(muted).child("Missing"))
                                })
                                .when(exists, |row| {
                                    let group = SharedString::from(format!("workspace-row-{i}"));
                                    let reveal = if cfg!(target_os = "macos") {
                                        "Show in Finder"
                                    } else {
                                        "Show in File Explorer"
                                    };
                                    let reveal_path = recent.path.clone();
                                    let reveal_weak = weak.clone();
                                    let open_path = recent.path.clone();
                                    let open_weak = weak.clone();
                                    row.group(group.clone()).child(
                                        // Only shown on the row under the pointer.
                                        h_flex()
                                            .flex_shrink_0()
                                            .opacity(0.)
                                            .group_hover(group, |s| s.opacity(1.))
                                            .child(
                                                Button::new(("reveal-workspace", i))
                                                    .ghost()
                                                    .xsmall()
                                                    .icon(IconName::FolderOpen)
                                                    .tooltip(reveal)
                                                    .accessibility_label(reveal)
                                                    .on_click(move |_, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.reveal_path(&reveal_path);
                                                        let _ = reveal_weak.update(cx, |this, cx| {
                                                            this.switcher_open = false;
                                                            cx.notify();
                                                        });
                                                    }),
                                            )
                                            // The current workspace is already in this window.
                                            .when(!current, |buttons| {
                                                buttons.child(
                                                    Button::new(("window-workspace", i))
                                                        .ghost()
                                                        .xsmall()
                                                        .icon(IconName::ExternalLink)
                                                        .tooltip("Open in new window")
                                                        .accessibility_label("Open in new window")
                                                        .on_click(move |_, window, cx| {
                                                            cx.stop_propagation();
                                                            let path = open_path.clone();
                                                            let _ = open_weak.update(cx, |this, cx| {
                                                                this.switch_workspace(
                                                                    path, true, window, cx,
                                                                )
                                                            });
                                                        }),
                                                )
                                            }),
                                    )
                                })
                                .when(!exists, |row| {
                                    let path = recent.path.clone();
                                    row.child(
                                        Button::new(("forget-workspace", i))
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::X)
                                            .tooltip("Remove from list")
                                            .accessibility_label("Remove from list")
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                workspaces::forget(&path, cx);
                                            }),
                                    )
                                })
                                .on_click(move |event: &ClickEvent, window, cx| {
                                    let modifiers = event.modifiers();
                                    let new_window = modifiers.control || modifiers.platform;
                                    let path = path.clone();
                                    let _ = weak.update(cx, |this, cx| {
                                        this.switch_workspace(path, new_window, window, cx)
                                    });
                                })
                        }))
                        .child(div().my_1().h(px(1.)).bg(theme.border))
                        .child({
                            let weak = weak.clone();
                            item("workspace-open".into())
                                .child(
                                    Icon::new(IconName::Plus)
                                        .size_3p5()
                                        .flex_shrink_0()
                                        .text_color(muted),
                                )
                                .child("Open new workspace…")
                                .on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.switcher_open = false;
                                        this.pick(true, window, cx);
                                    });
                                })
                        })
                })
        };
        let sidebar = v_flex()
            .w_full()
            .h_full()
            .flex_shrink_0()
            // Over a system backdrop the native sidebar material (tinted by the
            // desktop) shows through. Dark mode lets far more of the wallpaper
            // through, so only there it gets a veil of the sidebar color.
            .when(!backdrop, |sidebar| sidebar.bg(theme.sidebar))
            .when(backdrop && theme.mode.is_dark(), |sidebar| {
                sidebar.bg(theme.sidebar.opacity(0.6))
            })
            .border_r_1()
            .border_color(theme.border)
            // The title bar floats over the top of the sidebar; this
            // keeps the sidebar's own content below it.
            .when(CUSTOM_TITLEBAR, |sidebar| {
                sidebar.child(div().h(px(TITLEBAR_HEIGHT)).flex_shrink_0())
            })
            .child(
                h_flex()
                    // The workspace folder sits further left than the tree, so
                    // the tree's threads read as hanging from it.
                    .pl_2()
                    // Same right inset as the tree below, so the buttons end
                    // where a row's highlight does.
                    .pr_2()
                    .pt_4()
                    .pb_2()
                    .gap_2()
                    .justify_between()
                    .items_center()
                    .child(h_flex().flex_1().min_w_0().child(switcher))
                    .child(
                        h_flex()
                            .gap_1()
                            // Without a workspace there is nothing to refresh or
                            // add to: the one action is choosing a folder.
                            .when(self.root.is_none(), |row| {
                                row.child(
                                    Button::new("open-workspace")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Plus)
                                        .tooltip("Open new workspace")
                                        .accessibility_label("Open new workspace")
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.pick(true, w, cx)),
                                        ),
                                )
                            })
                            .when(self.root.is_some(), |row| {
                                row.child(
                                    Button::new("refresh")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::RefreshCw)
                                        .tooltip("Refresh folder")
                                        .accessibility_label("Refresh folder")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let paths: Vec<_> =
                                                this.expanded.iter().cloned().collect();
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
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.new_file(w, cx)),
                                        ),
                                )
                            }),
                    ),
            )
            .child(
                v_flex()
                    .id("files")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .children(rows.into_iter().enumerate().map(|(i, row)| {
                        let TreeRow {
                            entry,
                            threads,
                            last,
                            pending,
                        } = row;
                        let path = entry.path.clone();
                        let menu_path = path.clone();
                        let expanded = self.expanded.contains(&path);
                        let active = highlighted.as_ref() == Some(&path);
                        let label = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        // Tree threads use the text color, faded: light lines on
                        // dark, dark lines on light, and visible over the tint.
                        let line = theme.foreground.opacity(0.25);
                        let half = px(TREE_INDENT / 2.);
                        let mid = px(TREE_ROW_HEIGHT / 2.);
                        let depth = threads.len();
                        let renaming = pending || self.renaming.as_ref() == Some(&path);
                        let name_input = if pending {
                            self.filename.clone()
                        } else {
                            self.rename_input.clone()
                        };
                        // Threads: every row hangs off the workspace root. A
                        // straight line for each ancestor that continues below,
                        // and a rounded elbow into this row.
                        let guides = (0..=depth).map(|level| {
                            let own = level == depth;
                            let continues = threads.get(level).copied().unwrap_or(false);
                            div()
                                .relative()
                                .flex_shrink_0()
                                .w(px(TREE_INDENT))
                                .h_full()
                                .when(!own && continues, |col| {
                                    col.child(
                                        div()
                                            .absolute()
                                            .left(half)
                                            .top_0()
                                            .bottom_0()
                                            .border_l_1()
                                            .border_color(line),
                                    )
                                })
                                .when(own, |col| {
                                    col.child(
                                        div()
                                            .absolute()
                                            .left(half)
                                            .top_0()
                                            .w(half)
                                            .h(mid)
                                            .border_l_1()
                                            .border_b_1()
                                            .rounded_bl(px(6.))
                                            .border_color(line),
                                    )
                                    .when(!last, |col| {
                                        col.child(
                                            div()
                                                .absolute()
                                                .left(half)
                                                .top(mid)
                                                .bottom_0()
                                                .border_l_1()
                                                .border_color(line),
                                        )
                                    })
                                })
                        });
                        let muted = theme.muted_foreground;
                        h_flex()
                            .id(("file", i))
                            .h(px(TREE_ROW_HEIGHT))
                            .w_full()
                            .pl_1()
                            .pr_2()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            // Selection is a soft tint; the darker text carries it.
                            .when(active, |row| row.font_weight(FontWeight::MEDIUM))
                            .when(active || renaming, |row| {
                                row.bg(theme.secondary.opacity(0.5))
                            })
                            .when(!active && !renaming, |row| {
                                row.text_color(muted)
                                    .hover(|row| row.bg(theme.secondary.opacity(0.35)))
                            })
                            .children(guides)
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(16.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(entry.directory, |slot| {
                                        slot.child(
                                            Icon::new(if expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            })
                                            .size_3p5()
                                            .text_color(muted),
                                        )
                                    }),
                            )
                            .child(
                                Icon::new(if entry.directory {
                                    if expanded {
                                        IconName::FolderOpen
                                    } else {
                                        IconName::Folder
                                    }
                                } else {
                                    IconName::File
                                })
                                .size_4()
                                .flex_shrink_0()
                                .mr_1p5()
                                .text_color(muted),
                            )
                            .when(!renaming, |row| {
                                let path = path.clone();
                                row.child(
                                    // Only a double-click on the name itself renames;
                                    // the label is as wide as its text.
                                    div()
                                        .id(("file-name", i))
                                        .min_w_0()
                                        .truncate()
                                        .child(label)
                                        .on_click(cx.listener(
                                            move |this, event: &ClickEvent, w, cx| {
                                                if event.click_count() >= 2 {
                                                    cx.stop_propagation();
                                                    this.start_rename(path.clone(), w, cx);
                                                }
                                            },
                                        )),
                                )
                            })
                            .when(renaming, |row| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        // Keep presses in the field from opening the row.
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .capture_key_down(cx.listener(
                                            move |this, event: &KeyDownEvent, window, cx| {
                                                if event.keystroke.key == "escape" {
                                                    cx.stop_propagation();
                                                    if pending {
                                                        this.cancel_naming(window, cx);
                                                    } else {
                                                        this.cancel_rename(window, cx);
                                                    }
                                                }
                                            },
                                        ))
                                        // Borderless and unpadded, so the name is
                                        // edited where it sits on the row.
                                        .child(
                                            Input::new(&name_input)
                                                .xsmall()
                                                .appearance(false)
                                                .px_0()
                                                .py_0()
                                                .text_sm(),
                                        ),
                                )
                            })
                            .when(active, |row| row.text_color(theme.foreground))
                            .on_click(cx.listener(move |this, event: &ClickEvent, w, cx| {
                                if pending {
                                    return;
                                }
                                // The second click of a double-click only renames,
                                // and only on the name (see the label above).
                                if event.click_count() >= 2 {
                                    return;
                                }
                                this.selected = Some(path.clone());
                                // The tree takes focus, so Rename and Delete
                                // shortcuts act on the row just clicked.
                                this.focus.focus(w, cx);
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
                            .context_menu({
                                let weak = cx.entity().downgrade();
                                move |menu, _, cx| {
                                    if pending {
                                        return menu;
                                    }
                                    // Right-clicking a row selects it, as in Finder.
                                    let _ = weak.update(cx, |this, cx| {
                                        if this.selected.as_ref() != Some(&menu_path) {
                                            this.selected = Some(menu_path.clone());
                                            cx.notify();
                                        }
                                    });
                                    menu.item(
                                        PopupMenuItem::new("Rename")
                                            .icon(IconName::Pencil)
                                            .on_click({
                                                let weak = weak.clone();
                                                let path = menu_path.clone();
                                                move |_, window, cx| {
                                                    let _ = weak.update(cx, |this, cx| {
                                                        this.start_rename(path.clone(), window, cx)
                                                    });
                                                }
                                            }),
                                    )
                                    .item(PopupMenuItem::separator())
                                    .item(
                                        PopupMenuItem::new("Delete")
                                            .icon(IconName::Trash)
                                            .on_click({
                                                let weak = weak.clone();
                                                let path = menu_path.clone();
                                                move |_, _, cx| {
                                                    let _ = weak.update(cx, |this, cx| {
                                                        this.delete_entry(path.clone(), cx)
                                                    });
                                                }
                                            }),
                                    )
                                }
                            })
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
                        .when(document.markdown, |pane| {
                            markdown_commands(pane, &document.editor)
                        })
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
                                .text_sm()
                                .on_paste({
                                    let weak = cx.entity().downgrade();
                                    let editor = document.editor.clone();
                                    move |item, window, cx| {
                                        // Text pastes as usual. Image data (a screenshot)
                                        // or image files (copied in Finder) are saved into
                                        // the workspace and linked instead.
                                        let mut images = Vec::new();
                                        for entry in item.entries() {
                                            match entry {
                                                ClipboardEntry::Image(image) => {
                                                    images.push(PastedImage::Data(image.clone()))
                                                }
                                                ClipboardEntry::ExternalPaths(paths) => images.extend(
                                                    paths
                                                        .paths()
                                                        .iter()
                                                        .filter(|p| files::is_image(p))
                                                        .cloned()
                                                        .map(PastedImage::File),
                                                ),
                                                _ => {}
                                            }
                                        }
                                        if images.is_empty() {
                                            return false;
                                        }
                                        let editor = editor.clone();
                                        let _ = weak.update(cx, |this, cx| {
                                            this.paste_images(images, editor, window, cx)
                                        });
                                        true
                                    }
                                }),
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
                            .child(
                                div().size_full().child(
                                    // Spans the whole pane, like the editor, so its
                                    // scrollbar sits at the pane's right edge. The
                                    // padding is inside the scroll box: text starts
                                    // where the editor's does (Read and Edit swap in
                                    // place) and stops short of the overlaid bar.
                                    TextView::new(&document.preview)
                                        .selectable(true)
                                        .scrollable(true)
                                        .size_full()
                                        .pl(px(reader::TEXT_INSET_X))
                                        .pr_4()
                                        .pt(px(reader::TEXT_INSET_Y))
                                        .pb_8()
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
                                    .icon(IconName::Plus)
                                    .label("New note")
                                    .on_click(cx.listener(|this, _, w, cx| this.new_file(w, cx))),
                            ),
                    ),
            );
        }
        // The tab strip and view buttons: in Enso's title bar on macOS and
        // Windows, a bar of the same shape under the native one on Linux.
        let document_header = h_flex()
            .h(px(44.))
            // Fills the title bar's width beside the toggle; as a row of the
            // content column (Linux) it keeps its height instead of growing.
            .when(CUSTOM_TITLEBAR, |header| header.flex_1())
            .when(!CUSTOM_TITLEBAR, |header| header.w_full().flex_shrink_0())
            .min_w_0()
            .px_2()
            .gap_2()
            .items_center()
            .when(!CUSTOM_TITLEBAR, |header| {
                header.border_b_1().border_color(theme.border)
            })
            .child(tabs)
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
        let (custom_titlebar, standard_header) = if CUSTOM_TITLEBAR {
            let toggle_label = if self.sidebar_open {
                "Collapse sidebar"
            } else {
                "Expand sidebar"
            };
            // Keeps the document header lined up with the content column as the
            // sidebar opens, closes and resizes beneath the title bar.
            let toggle_width = 28.;
            let header_offset = (f32::from(sidebar_width) * sidebar_reveal
                - TRAFFIC_LIGHTS_WIDTH
                - toggle_width
                - 8.)
                .max(0.);
            // Transparent and borderless: it floats over the sidebar and the
            // content's own top strip, which draws the only horizontal border.
            let titlebar = TitleBar::new()
                .h(px(TITLEBAR_HEIGHT))
                .bg(gpui_kit::transparent_black())
                .border_color(gpui_kit::transparent_black())
                // Laid out on top of the bar rather than in it, so a full tab
                // strip scrolls within the window instead of widening the bar.
                .child(div().absolute().inset_0().child(
                    h_flex()
                        .size_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .pr_2()
                        .child(
                            // The title bar starts a window move on any press
                            // that then moves; keep presses on the toggle to
                            // itself so a slightly shaky click still toggles.
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                                ),
                        )
                        .child(div().flex_shrink_0().w(px(header_offset)))
                        .child(document_header),
                ));
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
            .relative()
            .size_full()
            // Over a translucent backdrop the window itself stays clear; the
            // content column paints its own opaque background.
            .when(!backdrop, |view| view.bg(theme.background))
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &OpenFile, w, cx| this.pick(false, w, cx)))
            .on_action(cx.listener(|this, _: &OpenFolder, w, cx| this.pick(true, w, cx)))
            .on_action(cx.listener(|this, _: &NewFile, w, cx| this.new_file(w, cx)))
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save(cx)))
            // Closes the current tab; with no tabs left, the window, as Cmd-W
            // does in other Mac apps (and quits after the last window).
            .on_action(cx.listener(|this, _: &CloseFile, window, cx| {
                if this.active.is_some() {
                    this.close_current_file(cx);
                } else {
                    this.request_close(CloseTarget::Window, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewFolder, w, cx| this.new_folder(w, cx)))
            // Only when the tree itself has focus: a text field passes Enter
            // and Cmd-Backspace on to here after handling them, and acting
            // again would reset a rename in progress (or delete a file).
            .on_action(cx.listener(|this, _: &RenameSelected, w, cx| {
                match this.selected_entry().filter(|_| this.focus.is_focused(w)) {
                    Some(path) => this.start_rename(path, w, cx),
                    None => cx.propagate(),
                }
            }))
            .on_action(cx.listener(|this, _: &DeleteSelected, w, cx| {
                match this.selected_entry().filter(|_| this.focus.is_focused(w)) {
                    Some(path) => this.delete_entry(path, cx),
                    None => cx.propagate(),
                }
            }))
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
                                // The sidebar starts at the panels' left edge, so the
                                // pointer's offset from it is the new width.
                                let width = (event.event.position.x - event.bounds.left())
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
                                            .on_drag(SidebarResize, |_, _, _, cx| {
                                                cx.new(|_| SidebarResize)
                                            }),
                                    )
                                }),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .bg(content_background(window, cx))
                                // Room for the floating title bar; its bottom
                                // border stops at the sidebar.
                                .when(custom_titlebar.is_some(), |view| {
                                    view.child(
                                        div()
                                            .h(px(TITLEBAR_HEIGHT))
                                            .flex_shrink_0()
                                            .border_b_1()
                                            .border_color(theme.border),
                                    )
                                })
                                .when_some(standard_header, |view, header| view.child(header))
                                .child(body)
                                .child(
                                    h_flex()
                                        .relative()
                                        .h(px(20.))
                                        .flex_shrink_0()
                                        .items_center()
                                        .overflow_hidden()
                                        .px_3()
                                        .py_0()
                                        .gap_3()
                                        .justify_end()
                                        .border_t_1()
                                        .border_color(theme.border)
                                        .text_size(px(10.))
                                        .line_height(relative(1.))
                                        .when(self.error && self.error_shown, |bar| {
                                            bar.child(
                                                div()
                                                    .absolute()
                                                    .inset_0()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_color(theme.danger)
                                                    .child(self.status.clone()),
                                            )
                                        })
                                        .child(
                                            div().text_color(theme.muted_foreground).child(stats),
                                        ),
                                ),
                        ),
                ),
            )
            .when_some(custom_titlebar, |view, titlebar| {
                view.child(div().absolute().top_0().left_0().right_0().child(titlebar))
            })
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

/// Markdown shortcuts, live only in a Markdown document's editor (its
/// `MarkdownEditor` context), not in the sidebar's name fields.
fn bind_markdown_keys(cx: &mut App) {
    let context = Some("MarkdownEditor > Input");
    macro_rules! key {
        ($keys:literal) => {
            if cfg!(target_os = "macos") {
                concat!("cmd-", $keys)
            } else {
                concat!("ctrl-", $keys)
            }
        };
    }
    cx.bind_keys([
        KeyBinding::new(key!("b"), Bold, context),
        KeyBinding::new(key!("i"), Italic, context),
        KeyBinding::new(key!("shift-x"), Strikethrough, context),
        KeyBinding::new(key!("shift-c"), InlineCode, context),
        KeyBinding::new(key!("k"), InsertLink, context),
        KeyBinding::new(key!("1"), Heading1, context),
        KeyBinding::new(key!("2"), Heading2, context),
        KeyBinding::new(key!("3"), Heading3, context),
        KeyBinding::new(key!("4"), Heading4, context),
        KeyBinding::new(key!("5"), Heading5, context),
        KeyBinding::new(key!("6"), Heading6, context),
        KeyBinding::new(key!("0"), BodyText, context),
        KeyBinding::new(key!("shift-8"), BulletList, context),
        KeyBinding::new(key!("shift-7"), NumberedList, context),
        KeyBinding::new(key!("shift-l"), TaskList, context),
        KeyBinding::new(key!("enter"), ToggleTaskDone, context),
        KeyBinding::new(key!("shift-."), Quote, context),
        KeyBinding::new(key!("alt-c"), CodeBlock, context),
        // These fall through to the editor's own Enter / Tab outside lists.
        KeyBinding::new("enter", ContinueList, context),
        KeyBinding::new("tab", IndentListItem, context),
        KeyBinding::new("shift-tab", OutdentListItem, context),
    ]);
}

/// Hook the Markdown shortcuts up to a document editor's pane.
fn markdown_commands(pane: Stateful<Div>, editor: &Entity<EditorState>) -> Stateful<Div> {
    use markdown_edit::{Block, Inline};
    // A command that always applies.
    macro_rules! command {
        ($pane:expr, $action:ty, $run:expr) => {{
            let editor = editor.clone();
            $pane.on_action(move |_: &$action, window, cx| {
                apply_markdown(&editor, window, cx, |text, selection| {
                    Some($run(text, selection))
                });
            })
        }};
    }
    // One that only applies to list lines, else the key does what it normally does.
    macro_rules! list_key {
        ($pane:expr, $action:ty, $run:expr) => {{
            let editor = editor.clone();
            $pane.on_action(move |_: &$action, window, cx| {
                if !apply_markdown(&editor, window, cx, $run) {
                    cx.propagate();
                }
            })
        }};
    }
    let inline = |kind| move |t: &str, s| markdown_edit::toggle_inline(t, s, kind);
    let block = |kind| move |t: &str, s| markdown_edit::toggle_block(t, s, kind);
    let pane = pane.key_context("MarkdownEditor");
    let pane = command!(pane, Bold, inline(Inline::Bold));
    let pane = command!(pane, Italic, inline(Inline::Italic));
    let pane = command!(pane, Strikethrough, inline(Inline::Strikethrough));
    let pane = command!(pane, InlineCode, inline(Inline::Code));
    let pane = command!(pane, InsertLink, markdown_edit::link);
    let pane = command!(pane, Heading1, block(Block::Heading(1)));
    let pane = command!(pane, Heading2, block(Block::Heading(2)));
    let pane = command!(pane, Heading3, block(Block::Heading(3)));
    let pane = command!(pane, Heading4, block(Block::Heading(4)));
    let pane = command!(pane, Heading5, block(Block::Heading(5)));
    let pane = command!(pane, Heading6, block(Block::Heading(6)));
    let pane = command!(pane, BodyText, block(Block::Heading(0)));
    let pane = command!(pane, BulletList, block(Block::Bullet));
    let pane = command!(pane, NumberedList, block(Block::Numbered));
    let pane = command!(pane, TaskList, block(Block::Task));
    let pane = command!(pane, Quote, block(Block::Quote));
    let pane = command!(pane, ToggleTaskDone, markdown_edit::toggle_task_done);
    let pane = command!(pane, CodeBlock, markdown_edit::toggle_code_block);
    let pane = list_key!(pane, ContinueList, |t: &str, s: std::ops::Range<usize>| {
        s.is_empty()
            .then(|| markdown_edit::continue_on_enter(t, s.start))
            .flatten()
    });
    let pane = list_key!(pane, IndentListItem, |t: &str, s| {
        markdown_edit::indent_list(t, s, false)
    });
    list_key!(pane, OutdentListItem, |t: &str, s| {
        markdown_edit::indent_list(t, s, true)
    })
}

/// Run a Markdown command on `editor`: hand it the lines around the selection
/// (not the whole document), then apply its edit as one undoable change.
/// Returns whether the command applied.
fn apply_markdown(
    editor: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut App,
    command: impl FnOnce(&str, std::ops::Range<usize>) -> Option<markdown_edit::Edit>,
) -> bool {
    editor.update(cx, |state, cx| {
        use gpui_kit::base::input::RopeExt as _;
        let selection = state.selected_range();
        let rope = state.text();
        // One line of context either side, for fences around a selection.
        let first = rope.offset_to_point(selection.start).row.saturating_sub(1);
        let last = rope.offset_to_point(selection.end).row + 1;
        let start = rope.line_start_offset(first);
        let end = rope.line_end_offset(last).max(selection.end);
        let text = rope.slice(start..end).to_string();
        let Some(edit) = command(&text, selection.start - start..selection.end - start) else {
            return false;
        };
        state.set_selected_range(edit.range.start + start..edit.range.end + start, cx);
        state.replace(edit.insert, window, cx);
        state.set_selected_range(edit.select.start + start..edit.select.end + start, cx);
        true
    })
}

fn open_enso_window(
    cx: &mut App,
    setup: impl FnOnce(&mut Enso, &mut Window, &mut Context<Enso>) + 'static,
) {
    // Cascade each new window off the ones already open.
    let offset = px(28. * open_views(cx).len() as f32);
    let mut bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
    bounds.origin.x += offset;
    bounds.origin.y += offset;
    let opened = gpui_kit::open_window(
        WindowOptions {
            titlebar: Some(if CUSTOM_TITLEBAR {
                TitlebarOptions {
                    traffic_light_position: Some(point(px(12.), px(TITLEBAR_HEIGHT / 2. - 7.))),
                    ..TitleBar::title_bar_options()
                }
            } else {
                TitlebarOptions {
                    title: Some("Enso".into()),
                    ..Default::default()
                }
            }),
            app_owns_titlebar_drag: CUSTOM_TITLEBAR,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(800.), px(500.))),
            window_background: window_backdrop(),
            ..Default::default()
        },
        cx,
        move |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                setup(&mut app, window, cx);
                app
            })
        },
    );
    match opened {
        // GPUI Kit's window root paints the theme background under everything,
        // which would hide the system backdrop; clear it so the translucent
        // sidebar shows through. Enso's content column paints its own.
        Ok((handle, _)) if translucent() => {
            let _ = handle.update(cx, |_, window, cx| {
                if let Some(Some(root)) = window.root::<gpui_kit::base::Root>() {
                    root.update(cx, |root, cx| {
                        root.style().background = Some(gpui_kit::transparent_black().into());
                        cx.notify();
                    });
                }
            });
        }
        Ok(_) => {}
        Err(error) => eprintln!("Unable to open an Enso window: {error}"),
    }
}

/// Open files and folders handed to Enso by the system (double-clicking a
/// Markdown file in Finder, Open With, dropping on the Dock icon).
fn open_urls(urls: Vec<String>, cx: &mut App) {
    for url in urls {
        let Some(path) = url::Url::parse(&url)
            .ok()
            .and_then(|url| url.to_file_path().ok())
        else {
            continue;
        };
        let views = open_views(cx);
        if path.is_dir() {
            match window_for(&path, EntityId::from(0u64), cx) {
                Some(handle) => activate(handle, cx),
                None => match views
                    .iter()
                    .find(|(_, view)| view.read(cx).root.is_none())
                    .cloned()
                {
                    // A window with no workspace yet takes the folder.
                    Some((handle, view)) => {
                        let _ = handle.update(cx, |_, window, cx| {
                            view.update(cx, |view, cx| view.open_folder(path, cx));
                            window.activate_window();
                        });
                    }
                    None => open_workspace_window(Some(path), cx),
                },
            }
            continue;
        }
        // A file goes to the window whose workspace holds it (the deepest one),
        // else the window in front, else a new window.
        let target = views
            .iter()
            .filter(|(_, view)| {
                view.read(cx)
                    .root
                    .as_ref()
                    .is_some_and(|root| path.starts_with(root))
            })
            .max_by_key(|(_, view)| {
                view.read(cx)
                    .root
                    .as_ref()
                    .map_or(0, |root| root.components().count())
            })
            .or_else(|| {
                let active = cx.active_window();
                views.iter().find(|(handle, _)| Some(*handle) == active)
            })
            .or(views.first())
            .cloned();
        match target {
            Some((handle, view)) => {
                let _ = handle.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.open_path(path, window, cx));
                    window.activate_window();
                });
            }
            None => open_enso_window(cx, move |app, window, cx| app.open_path(path, window, cx)),
        }
    }
}

fn open_workspace_window(root: Option<PathBuf>, cx: &mut App) {
    open_enso_window(cx, move |app, _, cx| {
        if let Some(root) = root {
            app.open_folder(root, cx);
        }
    });
}

fn main() {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    // The system can hand over files before the app has a context to open them
    // with; queue them until the run loop below picks them up.
    let (opened_tx, mut opened_rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = opened_tx.unbounded_send(urls);
    });
    app.run(move |cx| {
            gpui_kit::init(cx);
            app_icon::init();
            settings::init(cx);
            workspaces::init(cx);
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
            // Act on the selected tree row. Bound on the window, not in text
            // fields, so they never take over these keys while typing.
            cx.bind_keys(if cfg!(target_os = "macos") {
                [
                    KeyBinding::new("cmd-backspace", DeleteSelected, Some("Enso")),
                    KeyBinding::new("enter", RenameSelected, Some("Enso")),
                ]
            } else {
                [
                    KeyBinding::new("delete", DeleteSelected, Some("Enso")),
                    KeyBinding::new("f2", RenameSelected, Some("Enso")),
                ]
            });
            cx.bind_keys([
                KeyBinding::new("ctrl-shift-n", NewFolder, Some("Enso")),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-n", NewFolder, Some("Enso")),
            ]);
            bind_view_keys(cx);
            bind_markdown_keys(cx);
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
                        MenuItem::action("New Folder", NewFolder),
                        MenuItem::action("Close File", CloseFile),
                        #[cfg(not(target_os = "macos"))]
                        MenuItem::action("Settings…", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("Save", Save),
                    ],
                    disabled: false,
                },
            ]);
            cx.on_action(|_: &Quit, cx| cx.defer(quit_all));
            cx.on_window_closed(|cx, _| {
                let windows = cx.windows();
                cx.default_global::<OpenWindows>()
                    .0
                    .retain(|(handle, _)| windows.contains(handle));
                if open_views(cx).is_empty() {
                    cx.quit();
                }
            })
            .detach();
            if let Some(path) = initial {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                open_enso_window(cx, move |app, window, cx| app.open_path(path, window, cx));
            } else {
                match workspaces::last(cx) {
                    Some(last) if last.exists() => open_workspace_window(Some(last.path), cx),
                    Some(last) => open_enso_window(cx, move |app, _, cx| {
                        app.message(
                            format!("Your last workspace “{}” is no longer available.", last.name()),
                            true,
                            cx,
                        )
                    }),
                    None => open_enso_window(cx, |app, window, cx| {
                        app.open_first_launch_sample(window, cx)
                    }),
                }
            }
            cx.spawn(async move |cx| {
                use futures::StreamExt as _;
                while let Some(urls) = opened_rx.next().await {
                    cx.update(|cx| open_urls(urls, cx));
                }
            })
            .detach();
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
    fn close_file_with_no_tabs_closes_the_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
        });
        let (handle, _view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = Enso::new(window, cx);
                    view.add_document(Some(PathBuf::from("a.md")), "a".into(), None, window, cx);
                    view
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        // The first press closes the tab, the window stays.
        cx.simulate_keystrokes(handle, "ctrl-w");
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().contains(&handle)));
        // With no tabs left, the next one closes the window.
        cx.simulate_keystrokes(handle, "ctrl-w");
        cx.run_until_parked();
        assert!(!cx.update(|cx| cx.windows().contains(&handle)));
    }

    #[gpui_kit::test]
    fn only_recently_viewed_documents_keep_a_preview(cx: &mut TestAppContext) {
        cx.update(|cx| gpui_kit::init(cx));
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = Enso::new(window, cx);
                    for n in 0..5 {
                        view.add_document(
                            Some(PathBuf::from(format!("n{n}.md"))),
                            format!("# Note {n}"),
                            None,
                            window,
                            cx,
                        );
                    }
                    view
                })
            })
            .unwrap()
        });
        let view_tab = |cx: &mut TestAppContext, i: usize| {
            cx.update(|cx| view.update(cx, |view, cx| {
                view.active = Some(i);
                cx.notify();
            }));
            draw(cx, handle);
        };
        let loaded = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                view.read(cx)
                    .documents
                    .iter()
                    .map(|d| d.preview_loaded)
                    .collect::<Vec<_>>()
            })
        };
        draw(cx, handle);
        // Opening five files parsed only the one shown.
        assert_eq!(loaded(cx), [false, false, false, false, true]);
        for i in [0, 1, 2] {
            view_tab(cx, i);
        }
        // Three kept; the oldest (4) dropped its preview.
        assert_eq!(loaded(cx), [true, true, true, false, false]);
        // Coming back to it parses it again.
        view_tab(cx, 4);
        assert_eq!(loaded(cx), [false, true, true, false, true]);
        cx.update(|cx| {
            let text = view.read(cx).documents[4].preview.read(cx).rendered_text();
            assert!(text.as_str().contains("Note 4"), "{:?}", text.as_str());
        });
    }

    #[gpui_kit::test]
    fn tree_shortcuts_rename_and_clean_up_deleted_entries(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("enso-tree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let note = root.join("note.md");
        std::fs::write(&note, "hi").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            let (delete, rename) = if cfg!(target_os = "macos") {
                ("cmd-backspace", "enter")
            } else {
                ("delete", "f2")
            };
            cx.bind_keys([
                gpui_kit::KeyBinding::new(delete, super::DeleteSelected, Some("Enso")),
                gpui_kit::KeyBinding::new(rename, super::RenameSelected, Some("Enso")),
            ]);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = Enso::new(window, cx);
                    view.open_folder(root.clone(), cx);
                    view.selected = Some(note.clone());
                    view
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let keys = if cfg!(target_os = "macos") {
            ("cmd-backspace", "enter")
        } else {
            ("delete", "f2")
        };

        // Rename opens the name field on the selected row, which stays open
        // to type in, and Enter renames the file on disk.
        cx.simulate_keystrokes(handle, keys.1);
        cx.update(|cx| assert_eq!(view.read(cx).renaming.as_ref(), Some(&note)));
        draw(cx, handle);
        cx.update(|cx| assert_eq!(view.read(cx).renaming.as_ref(), Some(&note)));
        cx.simulate_input(handle, "renamed");
        cx.simulate_keystrokes(handle, "enter");
        cx.run_until_parked();
        assert!(root.join("renamed.md").exists(), "rename did not happen");
        std::fs::rename(root.join("renamed.md"), &note).unwrap();
        cx.update(|cx| view.update(cx, |view, _| view.selected = Some(note.clone())));

        // Forgetting a deleted entry closes its tab and clears the selection.
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.renaming = None;
                view.add_document(Some(note.clone()), "hi".into(), None, window, cx);
                view.forget_entry(&note, cx);
                assert!(view.documents.is_empty());
                assert!(view.selected.is_none());
            })
        })
        .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[gpui_kit::test]
    fn markdown_shortcuts_edit_and_fall_through(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            super::bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = Enso::new(window, cx);
                    view.add_document(
                        Some(PathBuf::from("note.md")),
                        "- one".into(),
                        None,
                        window,
                        cx,
                    );
                    view.mode = Mode::Edit;
                    view
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        let set = |cx: &mut TestAppContext, range: std::ops::Range<usize>| {
            cx.update_window(handle, |_, window, cx| {
                editor.focus_handle(cx).focus(window, cx);
                editor.update(cx, |e, cx| e.set_selected_range(range, cx));
            })
            .unwrap();
        };
        let text = |cx: &mut TestAppContext| cx.update(|cx| editor.read(cx).value().to_string());
        let primary = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };

        // Enter continues the list, Tab nests the new item, a second Enter on
        // the empty item steps it back out.
        set(cx, 5..5);
        cx.simulate_keystrokes(handle, "enter");
        assert_eq!(text(cx), "- one\n- ");
        cx.simulate_keystrokes(handle, "tab");
        assert_eq!(text(cx), "- one\n  - ");
        cx.simulate_keystrokes(handle, "enter");
        assert_eq!(text(cx), "- one\n- ");
        cx.simulate_keystrokes(handle, "enter");
        assert_eq!(text(cx), "- one\n");
        // Outside a list Enter and Tab keep their usual meaning.
        cx.simulate_input(handle, "word");
        cx.simulate_keystrokes(handle, "enter");
        assert_eq!(text(cx), "- one\nword\n");
        // Formatting shortcuts.
        set(cx, 6..10);
        cx.simulate_keystrokes(handle, &format!("{primary}-b"));
        assert_eq!(text(cx), "- one\n**word**\n");
        cx.simulate_keystrokes(handle, &format!("{primary}-2"));
        assert_eq!(text(cx), "- one\n## **word**\n");
        cx.simulate_keystrokes(handle, &format!("{primary}-z"));
        assert_eq!(text(cx), "- one\n**word**\n");
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
