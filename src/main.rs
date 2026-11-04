// Release builds on Windows open as a window, without a console behind it;
// debug builds keep the console for their log messages.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_icon;
mod brand;
mod diagram;
mod export;
mod files;
mod find;
mod http;
mod lock;
mod markdown_edit;
mod media;
mod onboarding;
mod outline;
mod palette;
mod reader;
mod rich;
mod search;
mod session;
mod settings;
mod spellcheck;
mod stats;
mod typography;
mod ui;
mod update;
mod workspaces;

use gpui_kit::assets::IconName;
use gpui_kit::base::{ResizeHandleContext, ResizeHandleState, Transition, transition};
extern crate gpui_kit as gpui;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Editor, EditorState, Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    tooltip::Tooltip,
    popover::Popover,
    resizable::{
        HandleEdge, h_resizable, resizable_panel, resize_handle, resize_handle_appearance,
    },
    text::{RenderedText, TextView, TextViewState},
    v_flex,
};
use gpui_kit::{prelude::FluentBuilder, *};
use ui::SurfaceExt as _;
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
        ReopenClosedTab,
        Save,
        ToggleView,
        ToggleSplit,
        SwapSplitSides,
        ToggleSidebar,
        NextTab,
        PreviousTab,
        NewFolder,
        RenameSelected,
        DeleteSelected,
        Quit,
        OpenCommandPalette,
        SearchEverything,
        PasteFiles,
        FindInPage,
        CopySelected,
        CutSelected,
        ExportNote,
        DismissExport,
        DismissLightbox,
        ToggleLock,
        CopyRichText,
        DismissCommandPalette,
        DismissRichControls,
        OpenInsertMenu,
        OpenSelectionInsertMenu
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
    outline: Vec<outline::Heading>,
    /// When this document was last the active one (`Enso::view_clock`).
    last_viewed: u64,
    saved: Arc<String>,
    dirty: bool,
    editor_loaded: bool,
    large_reader: Option<Entity<reader::LargeReader>>,
    stats: String,
    _stats_task: Option<Task<()>>,
    spellcheck: Option<Entity<spellcheck::Spellcheck>>,
    last_reader_cursor: Option<usize>,
    last_preview_text: Option<RenderedText>,
    _preview_task: Option<Task<()>>,
    _autosave_timer: Option<Task<()>>,
    revision: u64,
    saving: bool,
    close_after_save: bool,
    markdown: bool,
    /// The file's modified time and size when enso last matched it with
    /// `saved`; `None` until the first check. See `Enso::check_disk`.
    disk_stamp: Option<(std::time::SystemTime, u64)>,
    /// A "changed on disk" question is open for this document.
    disk_prompt: bool,
    /// Locked: readable, not changeable (see `lock`).
    locked: bool,
    /// The file was gone at the last check (already reported).
    missing: bool,
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
/// Widest a tab's file name gets before it ends in "…".
const TAB_TITLE_WIDTH: f32 = 200.;
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

/// The note's top-level blocks as source ranges, in the order the reader
/// lists them (one list item each): the same parser, with the reader's
/// options.
fn top_level_blocks(source: &str) -> Vec<std::ops::Range<usize>> {
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let Ok(tree) = markdown::to_mdast(source, &options) else {
        return Vec::new();
    };
    tree.children()
        .into_iter()
        .flatten()
        .filter_map(|node| node.position().map(|p| p.start.offset..p.end.offset))
        .collect()
}

/// The source offset under `point` in a reader: the block whose rows it's
/// on, then the line within that block at the same fraction of its height.
fn source_at_point(source: &str, list: &ListState, point: Point<Pixels>) -> Option<usize> {
    let blocks = top_level_blocks(source);
    // A reader that split the note differently can't be mapped this way.
    if blocks.len() != list.item_count() {
        return None;
    }
    let first = list.logical_scroll_top().item_ix;
    for ix in first..blocks.len() {
        let bounds = list.bounds_for_item(ix)?;
        if point.y > bounds.bottom() {
            continue;
        }
        let block = blocks[ix].clone();
        let fraction = ((point.y - bounds.top()) / bounds.size.height.max(px(1.))).clamp(0., 1.);
        let text = &source[block.clone()];
        let lines: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .filter(|&i| i < text.len())
            .collect();
        let line = ((fraction * lines.len() as f32) as usize).min(lines.len().saturating_sub(1));
        return Some(block.start + lines.get(line).copied().unwrap_or(0));
    }
    None
}

/// A tab being dragged along the tab strip. `from` is where it started; the
/// drag preview is the tab's name in a pill.
#[derive(Clone)]
struct DraggedTab {
    from: usize,
    title: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .h(px(28.))
            .px_2p5()
            .flex()
            .items_center()
            .rounded_md()
            .text_sm()
            .bg(theme.secondary)
            .text_color(theme.foreground)
            .opacity(0.9)
            .max_w(px(TAB_TITLE_WIDTH))
            .truncate()
            .child(self.title.clone())
    }
}

/// A toast on screen, or leaving it.
struct ToastEntry {
    toast: ui::toast::Toast,
    /// Which showing this is: an autohide timer only ends its own.
    serial: u64,
    leaving: bool,
}

/// Background work under way (see `Enso::busy`). Toasts that report its
/// outcome use the same `id`, so they replace its spinner.
struct Busy {
    id: SharedString,
    done: Arc<std::sync::atomic::AtomicBool>,
}

/// A sidebar entry being dragged; the preview is its icon and name.
#[derive(Clone)]
struct DraggedEntry {
    path: PathBuf,
    directory: bool,
    /// Everything that moves: the selection when the row is part of it.
    paths: Vec<PathBuf>,
}

impl Render for DraggedEntry {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        h_flex()
            .h(px(TREE_ROW_HEIGHT))
            .px_2()
            .gap_1p5()
            .items_center()
            .rounded_md()
            .text_sm()
            .bg(theme.secondary)
            .text_color(theme.foreground)
            .opacity(0.9)
            .child(
                Icon::new(if self.directory { IconName::Folder } else { IconName::File })
                    .size_4()
                    .text_color(theme.muted_foreground),
            )
            .child(div().max_w(px(TAB_TITLE_WIDTH)).truncate().child(name))
            .when(self.paths.len() > 1, |row| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("+{}", self.paths.len() - 1)),
                )
            })
    }
}

/// `paths` without the ones inside another of them, in order.
fn outermost(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in &paths {
        if !kept.contains(path) && !paths.iter().any(|other| other != path && path.starts_with(other)) {
            kept.push(path.clone());
        }
    }
    kept
}

/// Files copied or cut in the sidebar, with the text the clipboard got (all
/// it keeps on Linux and Windows, where paste recognises it as these files).
struct CopiedFiles {
    paths: Vec<PathBuf>,
    text: String,
    cut: bool,
}

struct ClosedTab {
    path: Option<PathBuf>,
    // Unsaved notes have no file to reload. Saved files stay lightweight.
    source: Option<String>,
    workspace: Option<PathBuf>,
}

struct Enso {
    focus: FocusHandle,
    documents: Vec<Document>,
    closed_tabs: Vec<ClosedTab>,
    /// The files last copied or cut in the sidebar.
    copied_files: Option<CopiedFiles>,
    /// Rows picked with Cmd/Ctrl- or Shift-click; two or more make the
    /// sidebar's selection (one is just `selected`).
    marked: Vec<PathBuf>,
    active: Option<usize>,
    root: Option<PathBuf>,
    folders: HashMap<PathBuf, Vec<files::Entry>>,
    /// Each listed folder's modified time when it was read: adding, removing
    /// or renaming an entry moves it, which is when the listing is redone.
    folder_stamps: HashMap<PathBuf, std::time::SystemTime>,
    expanded: HashSet<PathBuf>,
    mode: Mode,
    previous_mode: Mode,
    error: bool,
    naming: Option<NewEntry>,
    filename: Entity<InputState>,
    /// The tree row last clicked, file or folder; new entries go beside it.
    selected: Option<PathBuf>,
    palette: Option<palette::Palette>,
    controls: Option<rich::Controls>,
    closing_controls: Option<rich::Controls>,
    image_resize: Option<rich::ImageResize>,
    outline_hover: Option<(EntityId, f32)>,
    outline_bounds: Arc<std::sync::Mutex<Bounds<Pixels>>>,
    closing_palette: Option<palette::Palette>,
    switcher_open: bool,
    switcher_hovered: bool,
    window_handle: AnyWindowHandle,
    renaming: Option<PathBuf>,
    rename_input: Entity<InputState>,
    closing: Option<CloseTarget>,
    sidebar_open: bool,
    sidebar_width: Pixels,
    /// The active tab changed (opened, reopened, switched to or closed): the
    /// keyboard follows it on the next frame. Left on a tab that's no longer
    /// shown, keys would reach nothing, not even app shortcuts like Reopen.
    refocus: bool,
    hovered_tab: Option<EntityId>,
    /// The tab strip's scroll position, and the tab last scrolled into view.
    tab_scroll: ScrollHandle,
    scrolled_to_tab: Option<EntityId>,
    /// Counts document switches, to tell which documents were viewed last.
    view_clock: u64,
    warmed_doc: Option<EntityId>,
    /// The active file the sidebar last followed: switching to another one
    /// (a tab, the palette, a link) selects and reveals its row.
    followed_path: Option<PathBuf>,
    /// The workspace session last saved from this window, and whether the
    /// saved one is still being reopened (and must not be overwritten yet).
    saved_session: Option<(PathBuf, session::Session)>,
    restoring_session: bool,
    _disk_watch: Task<()>,
    /// Note texts kept between "Search everything" queries.
    search_cache: Arc<search::Cache>,
    /// A cursor position to show once its document is laid out: the
    /// document, the byte offset, and frames left to wait.
    pending_jump: Option<(EntityId, usize, u8)>,
    /// The match a search result opened at, highlighted once it's shown.
    pending_flash: Option<(EntityId, Vec<std::ops::Range<usize>>)>,
    flash_task: Option<Task<()>>,
    page_find: Option<find::PageFind>,
    /// While a tab is dragged: where it came from and the tab it's over.
    dragging_tab: Option<usize>,
    tab_drop: Option<usize>,
    /// While a file, folder or Finder item is dragged over the sidebar: the
    /// folder it would land in.
    tree_drop: Option<PathBuf>,
    /// Where the pointer was when a row last claimed the drop, so the list
    /// behind the rows doesn't take it back for the workspace root.
    tree_drop_point: Option<Point<Pixels>>,
    /// The Export dialog while open (and while it fades out), and the
    /// format and image choice it last ran with.
    export_dialog: Option<export::ExportDialog>,
    closing_export: Option<export::ExportDialog>,
    export_choice: export::ExportChoice,
    toasts: Vec<ToastEntry>,
    /// A picture shown full size (and the one fading out), with the focus
    /// that takes Escape.
    lightbox: Option<(media::Lightbox, FocusHandle)>,
    closing_lightbox: Option<(media::Lightbox, FocusHandle)>,
    toast_serial: u64,
    _subscriptions: Vec<Subscription>,
}

impl Enso {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("enso");
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
                    doc.preview.update(cx, |preview, cx| preview.invalidate_inline_layout(cx));
                    if let Some(reader) = &doc.large_reader {
                        reader.update(cx, |reader, cx| reader.refresh_layout(cx));
                    }
                }
                if !settings::document_outline(cx) { this.outline_hover = None; }
                // The split view's side order is a preference too.
                cx.notify();
            });
        Self {
            focus,
            documents: Vec::new(),
            closed_tabs: Vec::new(),
            copied_files: None,
            marked: Vec::new(),
            active: None,
            root: None,
            folders: HashMap::new(),
            folder_stamps: HashMap::new(),
            expanded: HashSet::new(),
            mode: Mode::Read,
            previous_mode: Mode::Read,
            error: false,
            naming: None,
            filename,
            selected: None,
            palette: None,
            controls: None,
            closing_controls: None,
            image_resize: None,
            outline_hover: None,
            outline_bounds: Arc::new(std::sync::Mutex::new(Bounds::default())),
            closing_palette: None,
            switcher_open: false,
            switcher_hovered: false,
            window_handle,
            renaming: None,
            rename_input,
            closing: None,
            sidebar_open: settings::sidebar_open(cx),
            sidebar_width: px(248.),
            refocus: false,
            hovered_tab: None,
            tab_scroll: ScrollHandle::new(),
            scrolled_to_tab: None,
            view_clock: 0,
            warmed_doc: None,
            followed_path: None,
            saved_session: None,
            restoring_session: false,
            _disk_watch: Self::watch_disk(window, cx),
            search_cache: Default::default(),
            pending_jump: None,
            pending_flash: None,
            flash_task: None,
            page_find: None,
            dragging_tab: None,
            tab_drop: None,
            tree_drop: None,
            tree_drop_point: None,
            export_dialog: None,
            closing_export: None,
            export_choice: Default::default(),
            toasts: Vec::new(),
            lightbox: None,
            closing_lightbox: None,
            toast_serial: 0,
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

    /// Whether the note behind `editor_id` is locked.
    pub(crate) fn is_locked(&self, editor_id: EntityId) -> bool {
        self.documents.iter().any(|d| d.editor.entity_id() == editor_id && d.locked)
    }

    /// Lock or unlock a note: its editor turns read-only, and every way of
    /// changing it from enso checks `is_editable` (or `is_locked`) first.
    fn set_locked(&mut self, index: usize, locked: bool, cx: &mut Context<Self>) {
        let doc = &mut self.documents[index];
        doc.locked = locked;
        doc.editor.update(cx, |editor, cx| editor.set_readonly(locked, cx));
        if let Some(path) = doc.path.clone() {
            lock::set_locked(&path, locked, cx);
        }
        cx.notify();
    }

    /// Move to the next tab (`step` 1) or the previous one (-1), wrapping
    /// around at either end, as clicking it would.
    pub(crate) fn cycle_tab(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.documents.len() as isize;
        if count < 2 {
            return;
        }
        let current = self.active.unwrap_or(0) as isize;
        self.active = Some((current + step).rem_euclid(count) as usize);
        self.selected = None;
        self.refocus = true;
        cx.notify();
    }

    /// Collapse or expand the sidebar, and remember it for every window and
    /// session.
    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        if let Err(error) = settings::set_sidebar_open(self.sidebar_open, cx) {
            eprintln!("enso: couldn't remember the sidebar: {error}");
        }
        cx.notify();
    }

    /// Lock Page / Unlock Page, for the active note.
    pub(crate) fn toggle_lock(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.active else {
            return;
        };
        let locked = !self.documents[index].locked;
        self.set_locked(index, locked, cx);
        let title = self.documents[index].title();
        let toast = if locked {
            ui::toast::done("page-lock", "Page locked", title)
        } else {
            ui::toast::done("page-lock", "Page unlocked", title)
        };
        self.toast(toast, cx);
    }

    /// The reader was clicked at `position`: put the editor's caret on the
    /// source behind it, so Split's editor scrolls there (and Edit opens
    /// there). A drag follows its selection's start, exactly; a plain click
    /// finds the block under the pointer and the line within it.
    fn reader_clicked(&mut self, editor_id: EntityId, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(index) = self.documents.iter().position(|d| d.editor.entity_id() == editor_id) else {
            return;
        };
        let doc = &self.documents[index];
        if !doc.markdown || !doc.editor_loaded || !doc.preview_loaded {
            return;
        }
        let preview = doc.preview.read(cx);
        let offset = preview.selected_source_range().map(|r| r.start).or_else(|| {
            let rendered = preview.rendered_text();
            source_at_point(rendered.source(), preview.list_state(), position)
        });
        let Some(offset) = offset else {
            return;
        };
        let doc = &mut self.documents[index];
        // The editor's caret moving would reveal it in the reader again;
        // the reader is already there.
        doc.last_reader_cursor = Some(offset);
        doc.editor.update(cx, |editor, cx| {
            let text = editor.text().clone();
            let mut at = offset.min(text.len());
            while at > 0 && !text.is_char_boundary(at) {
                at -= 1;
            }
            editor.set_selected_range(at..at, cx);
        });
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
            let (count, headings) = cx
                .background_executor()
                .spawn(async move {
                    let source = rope.map(|r| r.to_string()).unwrap_or_else(||saved.as_ref().clone());
                    (stats::word_count(source.chars()), outline::headings(&source))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(doc) = this
                    .documents
                    .iter_mut()
                    .find(|doc| doc.editor.entity_id() == editor_id && doc.revision == revision)
                {
                    doc.stats = stats::label(count);
                    doc.outline = headings;
                    cx.notify();
                }
            });
        }));
    }

    /// Report an error (as a toast), or note that things are fine again.
    /// `error` stays set until the next fine message: quitting relies on it
    /// to know whether auto-save failed. Messages that aren't errors show
    /// nothing; confirmations use `toast` directly.
    fn message(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        let message = message.into();
        self.error = error;
        if error && !message.is_empty() {
            self.toast(ui::toast::error(message), cx);
        }
        cx.notify();
    }

    /// Show a toast; one with the same id is replaced in place.
    fn toast(&mut self, toast: ui::toast::Toast, cx: &mut Context<Self>) {
        self.toast_serial += 1;
        let serial = self.toast_serial;
        let id = toast.id.clone();
        let autohide = toast.autohide;
        match self.toasts.iter_mut().find(|t| t.toast.id == id) {
            Some(entry) => {
                entry.toast = toast;
                entry.serial = serial;
                entry.leaving = false;
            }
            None => {
                self.toasts.push(ToastEntry { toast, serial, leaving: false });
                // At most four at once: the oldest gives way.
                let showing = self.toasts.iter().filter(|t| !t.leaving).count();
                if showing > 4 {
                    if let Some(oldest) = self.toasts.iter_mut().find(|t| !t.leaving) {
                        oldest.leaving = true;
                    }
                }
            }
        }
        if autohide {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(std::time::Duration::from_secs(4)).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(entry) = this.toasts.iter_mut().find(|t| t.serial == serial) {
                        entry.leaving = true;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn dismiss_toast(&mut self, id: &SharedString, cx: &mut Context<Self>) {
        if let Some(entry) = self.toasts.iter_mut().find(|t| t.toast.id == *id) {
            entry.leaving = true;
            cx.notify();
        }
    }

    /// The toasts, stacked at the bottom middle of the window, newest last.
    fn toast_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.toasts.is_empty() {
            return None;
        }
        let easing = cx.theme().motion_tokens().easing_move.clone();
        let mut shown = Vec::new();
        let mut gone = Vec::new();
        for entry in &self.toasts {
            let presence = transition(
                ElementId::from((ElementId::from(entry.toast.id.clone()), "toast")),
                if entry.leaving { 0.0_f32 } else { 1.0 },
                Transition::new(std::time::Duration::from_millis(220)).easing(easing.clone()),
                window,
                cx,
            )
            .clamp(0., 1.);
            if entry.leaving && presence <= 0.001 {
                gone.push(entry.serial);
                continue;
            }
            let weak = cx.entity().downgrade();
            let id = entry.toast.id.clone();
            shown.push(ui::toast::render(
                &entry.toast,
                presence,
                move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.dismiss_toast(&id, cx));
                },
                cx,
            ));
        }
        if !gone.is_empty() {
            self.toasts.retain(|t| !gone.contains(&t.serial));
        }
        Some(
            v_flex()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(36.))
                .px_4()
                .gap_2()
                .items_center()
                .children(shown)
                .into_any_element(),
        )
    }

    /// Work has started in the background: if it's still going after a
    /// moment, a spinner toast says so (quick work shows nothing). Pass the
    /// result to `finish`.
    fn busy(&mut self, id: impl Into<SharedString>, text: impl Into<SharedString>, cx: &mut Context<Self>) -> Busy {
        let busy = Busy { id: id.into(), done: Default::default() };
        let (id, done, text) = (busy.id.clone(), busy.done.clone(), text.into());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            if !done.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = this.update(cx, |this, cx| this.toast(ui::toast::working(id, text), cx));
            }
        })
        .detach();
        busy
    }

    /// The work behind `busy` is over: its spinner (if it came up) gives way
    /// to `then`, or just goes.
    fn finish(&mut self, busy: Busy, then: Option<ui::toast::Toast>, cx: &mut Context<Self>) {
        busy.done.store(true, std::sync::atomic::Ordering::SeqCst);
        match then {
            Some(toast) => self.toast(toast, cx),
            None => self.dismiss_toast(&busy.id, cx),
        }
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
            self.refocus = true;
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
        let spellcheck = markdown.then(|| cx.new(|cx| spellcheck::Spellcheck::new(&editor, source.len(), cx)));
        self.documents.push(Document {
            path,
            saved: source,
            dirty: false,
            editor_loaded: !large || !markdown,
            large_reader,
            stats,
            _stats_task: None,
            spellcheck,
            last_reader_cursor: None,
            last_preview_text: None,
            _preview_task: None,
            _autosave_timer: None,
            revision: 0,
            saving: false,
            close_after_save: false,
            disk_stamp: None,
            disk_prompt: false,
            missing: false,
            locked: false,
            editor,
            preview,
            preview_loaded: false,
            outline: Vec::new(),
            last_viewed: 0,
            markdown,
            _subscriptions: vec![subscription, cursor_subscription, preview_subscription],
        });
        self.active = Some(self.documents.len() - 1);
        self.refocus = true;
        if let Some(path) = self.documents.last().and_then(|d| d.path.clone()) {
            self.reveal_in_tree(&path, cx);
            if lock::is_locked(&path, cx) {
                self.set_locked(self.documents.len() - 1, true, cx);
            }
        }
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
            self.refocus = true;
            cx.notify();
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let busy = self.busy(format!("open:{}", path.display()), format!("Opening “{name}”…"), cx);
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
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish(busy, None, cx);
                match result {
                    Ok((path, source, sections)) => {
                        this.add_document(Some(path), source, sections, window, cx)
                    }
                    Err(error) => this.message(error.to_string(), true, cx),
                }
            });
        })
        .detach();
    }

    /// Open `path` (or switch to its tab) with the cursor at byte `offset`,
    /// `matches` (what a search found) highlighted for a moment.
    fn open_path_at(
        &mut self,
        path: PathBuf,
        offset: usize,
        matches: Vec<std::ops::Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.documents.iter().position(|d| d.path.as_ref() == Some(&path)) {
            self.active = Some(index);
            self.refocus = true;
            let id = self.documents[index].editor.entity_id();
            self.pending_jump = Some((id, offset, 120));
            self.pending_flash = (!matches.is_empty()).then(|| (id, matches));
            cx.notify();
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let busy = self.busy(format!("open:{}", path.display()), format!("Opening “{name}”…"), cx);
        let task = cx.background_executor().spawn({
            let path = path.clone();
            async move {
                files::read_text(&path).map(|text| {
                    let sections = (files::is_markdown(&path)
                        && text.len() >= reader::LARGE_DOCUMENT_BYTES)
                        .then(|| reader::LargeReader::index(&text));
                    (text, sections)
                })
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish(busy, None, cx);
                match result {
                Ok((text, sections)) => {
                    this.add_document(Some(path), text, sections, window, cx);
                    if let Some(doc) = this.active.and_then(|i| this.documents.get(i)) {
                        this.pending_jump = Some((doc.editor.entity_id(), offset, 120));
                        this.pending_flash = (!matches.is_empty()).then(|| (doc.editor.entity_id(), matches));
                    }
                }
                Err(error) => this.message(error.to_string(), true, cx),
                }
            });
        })
        .detach();
    }

    /// Run `pending_jump` once its document can show the position: the
    /// editor at once, the reader after its text has been laid out.
    fn run_pending_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, offset, frames)) = self.pending_jump else {
            return;
        };
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == id) else {
            self.pending_jump = None;
            return;
        };
        let ready = self.mode == Mode::Edit
            || doc.large_reader.is_some()
            || (doc.preview_loaded && !doc.preview.read(cx).rendered_text().source().is_empty());
        if ready || frames == 0 {
            self.pending_jump = None;
            let flash = self.pending_flash.take().filter(|(of, _)| *of == id).map(|(_, range)| range);
            cx.defer_in(window, move |this, window, cx| {
                this.jump_to_heading(id, offset, window, cx);
                if let Some(range) = flash {
                    this.flash_match(id, range, cx);
                }
            });
        } else {
            self.pending_jump = Some((id, offset, frames - 1));
            window.request_animation_frame();
        }
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
            .map(|n| format!("{} — enso", n.to_string_lossy()))
            .unwrap_or_else(|| "enso".into());
        let handle = self.window_handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.set_window_title(&title));
        });
        self.load_folder(path.clone(), cx);
        self.restore_session(path, cx);
    }

    /// Check the open files against the disk every so often. Polling, not
    /// file-system events: it works the same on network drives and in synced
    /// folders (iCloud, Dropbox), where events are unreliable, and a file is
    /// only read when its modified time or size moved.
    fn watch_disk(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1500))
                    .await;
                let Ok((open, shown)) = this.read_with(cx, |this, _| {
                    let open = this
                        .documents
                        .iter()
                        .filter(|d| !d.saving && !d.disk_prompt)
                        .filter_map(|d| {
                            Some((d.editor.entity_id(), d.path.clone()?, d.disk_stamp, d.saved.clone()))
                        })
                        .collect::<Vec<_>>();
                    (open, this.shown_folders())
                }) else {
                    return;
                };
                if !open.is_empty() {
                    let changes = cx
                        .background_executor()
                        .spawn(async move { files::changed_on_disk(open) })
                        .await;
                    if !changes.is_empty() {
                        let _ = this.update_in(cx, |this, window, cx| {
                            for change in changes {
                                this.apply_disk_change(change, window, cx);
                            }
                        });
                    }
                }
                if !shown.is_empty() {
                    let changed = cx
                        .background_executor()
                        .spawn(async move { files::changed_folders(shown) })
                        .await;
                    if !changed.is_empty() {
                        let _ = this.update(cx, |this, cx| this.refresh_folders(changed, cx));
                    }
                }
            }
        })
    }

    /// The folders the sidebar shows (the workspace and every expanded folder
    /// in it) with the stamp each was listed at. Only these are watched:
    /// a collapsed folder is read again when it's opened.
    fn shown_folders(&self) -> Vec<(PathBuf, Option<std::time::SystemTime>)> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        self.expanded
            .iter()
            .filter(|folder| folder.starts_with(root) && self.folders.contains_key(*folder))
            .map(|folder| (folder.clone(), self.folder_stamps.get(folder).copied()))
            .collect()
    }

    /// Folders that changed on disk: list them again; ones that are gone
    /// leave the sidebar (their parent's new listing drops their row).
    fn refresh_folders(&mut self, changed: Vec<(PathBuf, bool)>, cx: &mut Context<Self>) {
        for (folder, exists) in changed {
            if exists {
                self.load_folder(folder, cx);
            } else {
                self.folders.retain(|p, _| !p.starts_with(&folder));
                self.folder_stamps.retain(|p, _| !p.starts_with(&folder));
                self.expanded.retain(|p| !p.starts_with(&folder));
                cx.notify();
            }
        }
    }

    fn apply_disk_change(
        &mut self,
        change: files::DiskChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.documents.iter().position(|d| d.editor.entity_id() == change.id)
        else {
            return;
        };
        let doc = &mut self.documents[index];
        // A save started since the check: it will compare with the disk itself.
        if doc.saving {
            return;
        }
        let Some((stamp, text)) = change.now else {
            if !std::mem::replace(&mut doc.missing, true) {
                let title = doc.title();
                self.message(format!("“{title}” was moved or deleted outside enso."), true, cx);
            }
            return;
        };
        doc.missing = false;
        doc.disk_stamp = Some(stamp);
        let Some(text) = text else {
            return; // Same text as enso's copy, only the timestamp moved.
        };
        if !doc.dirty {
            self.reload_document(index, text, window, cx);
            return;
        }
        // Edits here that the file doesn't have yet, and a different file:
        // the user picks which one wins.
        doc.disk_prompt = true;
        let title = doc.title();
        let editor_id = change.id;
        let receiver = window.prompt(
            PromptLevel::Warning,
            &format!("“{title}” changed on disk"),
            Some("Another app changed this file while you had unsaved edits in enso."),
            &["Keep My Version", "Use the Version on Disk"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(index) =
                    this.documents.iter().position(|d| d.editor.entity_id() == editor_id)
                else {
                    return;
                };
                this.documents[index].disk_prompt = false;
                if answer == Ok(1) {
                    this.reload_document(index, text, window, cx);
                } else {
                    // Overwrite the file with the edits: the next save expects
                    // the disk's text, so its conflict check passes.
                    this.documents[index].saved = Arc::new(text);
                    this.start_save(index, cx);
                }
            });
        })
        .detach();
    }

    /// Replace a document's text with what's on disk, keeping the cursor.
    fn reload_document(
        &mut self,
        index: usize,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let doc = &mut self.documents[index];
        let text = Arc::new(text);
        doc.saved = text.clone();
        doc.dirty = false;
        doc._autosave_timer = None;
        // `set_value` raises no change event, so this isn't taken for an edit;
        // the preview and word count are refreshed here instead.
        if doc.editor_loaded || doc.large_reader.is_none() {
            doc.editor.update(cx, |editor, cx| {
                let cursor = editor.cursor().min(text.len());
                editor.set_value(text.as_str(), window, cx);
                if text.is_char_boundary(cursor) {
                    editor.set_selected_range(cursor..cursor, cx);
                }
            });
        }
        if let Some(reader) = doc.large_reader.clone() {
            let source = text.clone();
            let task = cx
                .background_executor()
                .spawn(async move { reader::LargeReader::index(&source) });
            let source = text.clone();
            cx.spawn(async move |_, cx| {
                let sections = task.await;
                let _ = reader.update(cx, |reader, cx| reader.replace(source, sections, cx));
            })
            .detach();
        }
        if doc.preview_loaded && doc.large_reader.is_none() {
            doc.preview.update(cx, |preview, cx| preview.set_text(&text, cx));
        }
        doc.revision += 1;
        self.schedule_word_count(index, true, cx);
        let title = self.documents[index].title();
        self.toast(
            ui::toast::done(SharedString::from(format!("reloaded:{title}")), format!("Reloaded “{title}”"), "It changed outside enso."),
            cx,
        );
        cx.notify();
    }

    /// Expand the folders between the workspace and `path`, so a file opened
    /// from deep inside the workspace shows in the sidebar.
    fn reveal_in_tree(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut folders: Vec<_> = path
            .ancestors()
            .skip(1)
            .take_while(|folder| folder.starts_with(&root) && *folder != root)
            .map(std::path::Path::to_path_buf)
            .collect();
        folders.reverse();
        for folder in folders {
            if self.expanded.insert(folder.clone()) {
                self.load_folder(folder, cx);
            }
        }
    }

    /// This window's workspace session as it stands: its saved files in tab
    /// order, the active one, the view mode and the sidebar.
    fn current_session(&self) -> Option<(PathBuf, session::Session)> {
        let root = self.root.clone()?;
        // A tab only waiting to finish saving before it closes isn't part of it.
        let saved: Vec<_> = self
            .documents
            .iter()
            .filter(|d| d.path.is_some() && !d.close_after_save)
            .collect();
        let active = self.active.and_then(|i| {
            let id = self.documents[i].editor.entity_id();
            saved.iter().position(|d| d.editor.entity_id() == id)
        });
        Some((
            root,
            session::Session {
                tabs: saved.iter().filter_map(|d| d.path.clone()).collect(),
                active,
                mode: match self.mode {
                    Mode::Read => "read",
                    Mode::Edit => "edit",
                    Mode::Split => "split",
                }
                .into(),
                sidebar_open: self.sidebar_open,
                sidebar_width: f32::from(self.sidebar_width),
                // Sorted: `expanded` is a set, and an unchanged session must
                // compare equal so it isn't written again.
                expanded: {
                    let mut folders: Vec<_> =
                        self.expanded.iter().filter(|p| self.root.as_ref() != Some(*p)).cloned().collect();
                    folders.sort();
                    folders
                },
            },
        ))
    }

    fn save_session(&mut self, cx: &mut App) {
        if self.restoring_session {
            return;
        }
        if let Some(current) = self.current_session() {
            if self.saved_session.as_ref() != Some(&current) {
                session::save(&current.0, current.1.clone(), cx);
                self.saved_session = Some(current);
            }
        }
    }

    /// Reopen what `root` had open last time: the view and sidebar at once,
    /// the tabs once their files are read, in their old order.
    fn restore_session(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let Some(saved) = session::get(&root, cx) else {
            return;
        };
        let mode = match saved.mode.as_str() {
            "edit" => Mode::Edit,
            "split" => Mode::Split,
            _ => Mode::Read,
        };
        // Open or collapsed is one app-wide choice (see `toggle_sidebar`),
        // not the workspace's; only its width is the workspace's own.
        self.sidebar_width = px(saved.sidebar_width.clamp(160., 480.));
        for folder in &saved.expanded {
            if folder.starts_with(&root) && folder.is_dir() && self.expanded.insert(folder.clone()) {
                self.load_folder(folder.clone(), cx);
            }
        }
        if saved.tabs.is_empty() {
            return;
        }
        self.restoring_session = true;
        let count = saved.tabs.len();
        let busy = self.busy(
            format!("restore:{}", root.display()),
            if count == 1 { "Opening your note…".to_string() } else { format!("Opening your {count} notes…") },
            cx,
        );
        let tabs = saved.tabs.clone();
        let task = cx.background_executor().spawn(async move {
            tabs.into_iter()
                .map(|path| {
                    // Moved or deleted files are skipped.
                    let loaded = files::read_text(&path).ok().map(|text| {
                        let sections = (files::is_markdown(&path)
                            && text.len() >= reader::LARGE_DOCUMENT_BYTES)
                            .then(|| reader::LargeReader::index(&text));
                        (text, sections)
                    });
                    (path, loaded)
                })
                .collect::<Vec<_>>()
        });
        let handle = self.window_handle;
        cx.spawn(async move |this, cx| {
            let files = task.await;
            let _ = handle.update(cx, |_, window, cx| {
                this.update(cx, |this, cx| {
                    this.finish(busy, None, cx);
                    this.restoring_session = false;
                    // The user moved on to another workspace meanwhile.
                    if this.root.as_ref() != Some(&root) {
                        return;
                    }
                    let mut active = None;
                    for (i, (path, loaded)) in files.into_iter().enumerate() {
                        let Some((text, sections)) = loaded else {
                            continue;
                        };
                        this.add_document(Some(path.clone()), text, sections, window, cx);
                        if saved.active == Some(i) {
                            active = this.documents.iter().position(|d| d.path.as_ref() == Some(&path));
                        }
                    }
                    if let Some(active) = active.or(this.active) {
                        this.active = Some(active);
                    }
                    // Opening a file shows it in Read; set_mode also fills the
                    // editor that Edit and Split show.
                    this.set_mode(mode, window, cx);
                    cx.notify();
                })
            });
        })
        .detach();
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
        // pending edits first, as closing a tab by hand does. Remember them
        // first, to bring them back when this workspace is opened again.
        self.save_session(cx);
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
        let task = cx.background_executor().spawn(async move {
            // Stamped before reading, so a change made while it's read is
            // still seen by the next check.
            let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            (path.clone(), stamp, files::list_folder(&path))
        });
        cx.spawn(async move |this, cx| {
            let (path, stamp, result) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.root != root {
                    return;
                }
                match result {
                    Ok(entries) => {
                        if let Some(stamp) = stamp {
                            this.folder_stamps.insert(path.clone(), stamp);
                        }
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
        if self.is_locked(editor.entity_id()) {
            self.message("This page is locked. Unlock it to add images.", true, cx);
            return;
        }
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
        // Saving the images happens off the window's thread (a big
        // screenshot takes a moment), then the links go in where the cursor
        // was when they were pasted.
        let selection = editor.read(cx).selected_range();
        let count = images.len();
        let busy = self.busy(
            format!("images:{}", editor.entity_id()),
            if count == 1 { "Adding the image…".to_string() } else { format!("Adding {count} images…") },
            cx,
        );
        let task = cx.background_executor().spawn(async move {
            images
                .into_iter()
                .map(|image| {
                    let saved = match &image {
                        PastedImage::Data(image) => {
                            files::save_image(&root, None, &image.bytes, image.format.extension())
                        }
                        PastedImage::File(path) => files::import_image(&root, path),
                    }?;
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
                    Ok(format!(
                        "<img src=\"{}\" alt=\"{}\" width=\"100%\" />",
                        attr(&link),
                        attr(&alt)
                    ))
                })
                .collect::<Vec<std::io::Result<String>>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let results = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish(busy, None, cx);
                let mut links = Vec::new();
                for result in results {
                    match result {
                        Ok(link) => links.push(link),
                        Err(error) => {
                            this.message(format!("Could not save the image: {error}"), true, cx)
                        }
                    }
                }
                if !links.is_empty() {
                    editor.update(cx, |editor, cx| {
                        let text = editor.text().clone();
                        let len = text.len();
                        let start = selection.start.min(len);
                        let end = selection.end.clamp(start, len);
                        if text.is_char_boundary(start) && text.is_char_boundary(end) {
                            editor.set_selected_range(start..end, cx);
                        }
                        editor.replace(links.join("\n"), window, cx)
                    });
                }
            });
        })
        .detach();
    }

    /// Files dropped on the document area: images are inserted into the open
    /// note at its cursor (as pasting them would), other files open as tabs.
    fn drop_files(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let (images, others): (Vec<_>, Vec<_>) =
            paths.into_iter().partition(|path| files::is_image(path));
        if !images.is_empty() {
            match self.active.map(|i| &self.documents[i]).filter(|d| d.markdown) {
                Some(doc) => {
                    let editor = doc.editor.clone();
                    let images = images.into_iter().map(PastedImage::File).collect();
                    self.paste_images(images, editor, window, cx);
                }
                None => self.message("Open a note to drop images into it.", true, cx),
            }
        }
        for path in others {
            if path.is_file() {
                self.open_path(path, window, cx);
            }
        }
    }

    /// The tree row last clicked, if it is still inside the open workspace.
    /// The sidebar's selection: the picked rows, or the one selected row.
    /// What's inside a selected folder goes with the folder, not on its own.
    fn selection(&self) -> Vec<PathBuf> {
        let Some(root) = &self.root else { return Vec::new() };
        let picked: Vec<PathBuf> = if self.marked.len() > 1 {
            self.marked
                .iter()
                .filter(|p| *p != root && p.starts_with(root) && p.exists())
                .cloned()
                .collect()
        } else {
            self.selected_entry().into_iter().collect()
        };
        outermost(picked)
    }

    /// What an action on `path`'s row works on: the whole selection when
    /// the row is part of it, else just that row.
    fn selection_with(&self, path: &PathBuf) -> Vec<PathBuf> {
        if self.marked.len() > 1 && self.marked.contains(path) {
            self.selection()
        } else {
            vec![path.clone()]
        }
    }

    /// A Cmd/Ctrl-click (`toggle`) adds the row to the selection or takes it
    /// out; a Shift-click picks every row from the selected one to it, when
    /// both are in the same folder. False when the click is a plain one.
    fn pick_row(&mut self, path: &PathBuf, shift: bool, toggle: bool) -> bool {
        if toggle {
            if self.marked.is_empty() {
                self.marked.extend(self.selected_entry());
            }
            if let Some(i) = self.marked.iter().position(|p| p == path) {
                self.marked.remove(i);
                if self.selected.as_ref() == Some(path) {
                    self.selected = self.marked.last().cloned();
                }
            } else {
                self.marked.push(path.clone());
                self.selected = Some(path.clone());
            }
            return true;
        }
        if !shift {
            return false;
        }
        let Some(anchor) = self.selected_entry() else { return false };
        // Ranges stay within one folder.
        if anchor.parent() != path.parent() {
            return false;
        }
        let Some(siblings) = path.parent().and_then(|p| self.folders.get(p)) else {
            return false;
        };
        let at = |p: &PathBuf| siblings.iter().position(|e| &e.path == p);
        let (Some(a), Some(b)) = (at(&anchor), at(path)) else { return false };
        // The selected row stays the anchor, so another Shift-click extends
        // or shrinks the range from it.
        self.marked = siblings[a.min(b)..=a.max(b)].iter().map(|e| e.path.clone()).collect();
        true
    }

    fn selected_entry(&self) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        self.selected
            .clone()
            .filter(|path| path != root && path.starts_with(root) && path.exists())
    }

    /// Delete from the sidebar or the palette: to the Trash, after a system
    /// prompt unless Settings › Confirm Before Deleting is off.
    fn delete_entry(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !settings::confirm_delete(cx) {
            self.trash_entry(path, cx);
            return;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        let detail = if path.is_dir() {
            format!("The folder and everything in it go to the {bin}.")
        } else {
            format!("You can restore it from the {bin}.")
        };
        let receiver = window.prompt(
            PromptLevel::Warning,
            &format!("Delete “{name}”?"),
            Some(&detail),
            &[format!("Move to {bin}").as_str(), "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if receiver.await == Ok(0) {
                let _ = this.update(cx, |this, cx| this.trash_entry(path, cx));
            }
        })
        .detach();
    }

    /// Delete several sidebar items at once, after one prompt.
    fn delete_entries(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if paths.len() <= 1 {
            if let Some(path) = paths.into_iter().next() {
                self.delete_entry(path, window, cx);
            }
            return;
        }
        if !settings::confirm_delete(cx) {
            self.trash_entries(paths, cx);
            return;
        }
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        let receiver = window.prompt(
            PromptLevel::Warning,
            &format!("Delete {} items?", paths.len()),
            Some(&format!("You can restore them from the {bin}.")),
            &[format!("Move to {bin}").as_str(), "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if receiver.await == Ok(0) {
                let _ = this.update(cx, |this, cx| this.trash_entries(paths, cx));
            }
        })
        .detach();
    }

    /// Move a file or folder to the Trash (Recycle Bin on Windows). Only an
    /// item that can't be recycled is ever deleted outright, and then the
    /// system asks first (Windows) or the move fails and nothing is deleted
    /// (macOS, Linux).
    fn trash_entry(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.trash_entries(vec![path], cx);
    }

    /// Trash each item, then say once what went: "Moved “note.md” to the
    /// Trash", or "Moved 3 items to the Trash".
    fn trash_entries(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut gone = Vec::new();
        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // On the main thread: Windows' "delete permanently?" dialog needs it.
            match files::move_to_trash(&path) {
                Ok(()) => {
                    self.forget_entry(&path, cx);
                    gone.push(name);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    self.toast(ui::toast::done(SharedString::from(format!("kept:{name}")), format!("Kept “{name}”"), error.to_string()), cx)
                }
                Err(error) => self.message(format!("Could not delete “{name}”: {error}"), true, cx),
            }
        }
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        let message = match gone.as_slice() {
            [] => return,
            [name] => format!("Moved “{name}” to the {bin}"),
            _ => format!("Moved {} items to the {bin}", gone.len()),
        };
        self.toast(ui::toast::done("trashed", message, ""), cx);
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
        self.marked.retain(|p| !p.starts_with(path));
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

    /// Rename a note from anywhere (the command palette): its row in the
    /// sidebar, shown and opened up to it, takes the new name inline.
    pub(crate) fn rename_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !self.root.as_ref().is_some_and(|root| path.starts_with(root)) {
            self.message("Open its folder as the workspace to rename it.", true, cx);
            return;
        }
        self.sidebar_open = true;
        self.reveal_in_tree(&path, cx);
        self.selected = Some(path.clone());
        self.start_rename(path, window, cx);
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
            Ok(new) if new != old => self.entry_moved(&old, &new, cx),
            Ok(_) => {}
            Err(error) => self.message(error.to_string(), true, cx),
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// A file or folder went from `old` to `new` (renamed or moved): open
    /// tabs, the selection and expanded folders follow it, and both folders
    /// it touched are listed again.
    fn entry_moved(&mut self, old: &PathBuf, new: &PathBuf, cx: &mut Context<Self>) {
        lock::moved(old, new, cx);
        let moved = |path: &PathBuf| {
            path.strip_prefix(old)
                .ok()
                .map(|rest| if rest.as_os_str().is_empty() { new.clone() } else { new.join(rest) })
        };
        if let Some(path) = self.selected.as_ref().and_then(|p| moved(p)) {
            self.selected = Some(path);
        }
        for path in &mut self.marked {
            if let Some(new) = moved(path) {
                *path = new;
            }
        }
        for doc in &mut self.documents {
            if let Some(path) = doc.path.as_ref().and_then(|p| moved(p)) {
                doc.markdown = files::is_markdown(&path);
                doc.path = Some(path);
            }
        }
        self.folders.retain(|path, _| !path.starts_with(old));
        self.expanded = self
            .expanded
            .drain()
            .map(|path| moved(&path).unwrap_or(path))
            .collect();
        let reload: Vec<_> = old
            .parent()
            .into_iter()
            .chain(new.parent())
            .map(PathBuf::from)
            .chain(self.expanded.iter().filter(|p| p.starts_with(new)).cloned())
            .collect();
        for path in reload {
            self.load_folder(path, cx);
        }
    }

    /// Drag and drop in the sidebar: move a file or folder into `dir`.
    fn move_entry(&mut self, path: PathBuf, dir: PathBuf, cx: &mut Context<Self>) -> Option<PathBuf> {
        self.tree_drop = None;
        // Let go where it already is (or on itself): nothing to do.
        if path.parent() == Some(dir.as_path()) || path == dir {
            return None;
        }
        let new = match files::move_into(&path, &dir) {
            Ok(new) => {
                if self.root.as_ref() != Some(&dir) && self.expanded.insert(dir.clone()) {
                    self.load_folder(dir, cx);
                }
                self.entry_moved(&path, &new, cx);
                self.selected = Some(new.clone());
                Some(new)
            }
            Err(error) => {
                self.message(error.to_string(), true, cx);
                None
            }
        };
        cx.notify();
        new
    }

    /// Move several sidebar items into `dir`; they stay selected there.
    fn move_entries(&mut self, paths: Vec<PathBuf>, dir: PathBuf, cx: &mut Context<Self>) {
        let paths = outermost(paths);
        if paths.len() == 1 {
            self.move_entry(paths[0].clone(), dir, cx);
            return;
        }
        // Moving a folder into one of the items being moved isn't possible.
        if paths.iter().any(|p| dir.starts_with(p)) {
            self.tree_drop = None;
            self.message("A folder can't be moved into itself.", true, cx);
            return;
        }
        let moved: Vec<PathBuf> = paths
            .into_iter()
            .map(|path| self.move_entry(path.clone(), dir.clone(), cx).unwrap_or(path))
            .collect();
        self.marked = moved;
        cx.notify();
    }

    /// A drag over the sidebar is over a row (or the list behind the rows)
    /// whose drop folder is `dir`.
    fn hover_tree_drop(
        &mut self,
        bounds: Bounds<Pixels>,
        point: Point<Pixels>,
        dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if bounds.contains(&point) {
            self.tree_drop_point = Some(point);
            if self.tree_drop.as_ref() != Some(&dir) {
                self.tree_drop = Some(dir);
                cx.notify();
            }
        }
    }

    /// Files or folders from Finder (dropped or pasted) copied into `dir`.
    fn copy_into_folder(&mut self, paths: Vec<PathBuf>, dir: PathBuf, cx: &mut Context<Self>) {
        self.tree_drop = None;
        if !self.root.as_ref().is_some_and(|root| dir.starts_with(root)) || paths.is_empty() {
            return;
        }
        let target = dir.clone();
        let count = paths.len();
        let busy = self.busy(
            format!("copy-in:{}", dir.display()),
            if count == 1 { "Copying 1 item…".to_string() } else { format!("Copying {count} items…") },
            cx,
        );
        let task = cx.background_executor().spawn(async move {
            paths
                .iter()
                .map(|path| files::copy_into(path, &target))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish(busy, None, cx);
                let mut copied = Vec::new();
                for result in results {
                    match result {
                        Ok(path) => copied.push(path),
                        Err(error) => this.message(error.to_string(), true, cx),
                    }
                }
                // Show the copies: the folder and the folders above it open.
                this.reveal_in_tree(&dir, cx);
                this.expanded.insert(dir.clone());
                this.load_folder(dir, cx);
                if let Some(first) = copied.first() {
                    this.selected = Some(first.clone());
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Cmd/Ctrl+V in the sidebar: files copied in Finder go into the selected
    /// folder (or beside the selected file).
    /// Copy (or cut) the sidebar's selected file or folder, only while the
    /// sidebar has focus: in the editor or reader the keys work on text as
    /// usual. The file itself goes on the clipboard, so Finder can paste a
    /// copy. A cut item fades until it's pasted, which moves it.
    fn copy_selected(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selection();
        if paths.is_empty() || !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let text = paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n");
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![
                ClipboardEntry::ExternalPaths(ExternalPaths(paths.iter().cloned().collect())),
                ClipboardEntry::String(ClipboardString::new(text.clone())),
            ],
        });
        // Where the system clipboard keeps only text (Linux, Windows), paste
        // recognises that text as these files.
        self.copied_files = Some(CopiedFiles { paths, text, cut });
        cx.notify();
    }

    fn paste_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let Some(dir) = self.target_dir() else {
            return;
        };
        let item = cx.read_from_clipboard();
        let mut paths: Vec<PathBuf> = item
            .as_ref()
            .map(|item| {
                item.entries()
                    .iter()
                    .filter_map(|entry| match entry {
                        ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                        _ => None,
                    })
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();
        // Files copied in the sidebar, on a clipboard that kept only their text.
        if paths.is_empty()
            && let Some(copied) = &self.copied_files
            && item.as_ref().and_then(|i| i.text()).as_deref() == Some(copied.text.as_str())
        {
            paths = copied.paths.clone();
        }
        // Cut in the sidebar: pasting moves them, once.
        if let Some(copied) = self.copied_files.take_if(|c| c.cut && c.paths == paths) {
            self.move_entries(copied.paths, dir, cx);
            cx.write_to_clipboard(ClipboardItem { entries: Vec::new() });
            return;
        }
        if paths.is_empty() {
            cx.propagate();
            return;
        }
        self.copy_into_folder(paths, dir, cx);
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

    /// Move the tab at `from` to `to`, keeping the same document active.
    fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.documents.len() || to >= self.documents.len() {
            return;
        }
        let active = self.active.map(|i| self.documents[i].editor.entity_id());
        let doc = self.documents.remove(from);
        self.documents.insert(to, doc);
        self.active = active.and_then(|id| {
            self.documents.iter().position(|doc| doc.editor.entity_id() == id)
        });
        cx.notify();
    }

    /// Insertion boundary under the pointer, including gaps and trailing space.
    /// ScrollHandle stores item bounds before its scroll offset is applied.
    fn tab_insertion_slot(&self, x: Pixels) -> usize {
        let offset = self.tab_scroll.offset().x;
        (0..self.documents.len())
            .find(|&i| self.tab_scroll.bounds_for_item(i).is_some_and(|bounds| {
                x < bounds.center().x + offset
            }))
            .unwrap_or(self.documents.len())
    }

    fn remove_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.hovered_tab == Some(self.documents[index].editor.entity_id()) {
            self.hovered_tab = None;
        }
        let doc = self.documents.remove(index);
        self.closed_tabs.push(ClosedTab {
            path: doc.path.clone(),
            source: doc.path.is_none().then(|| doc.editor.read(cx).value().to_string()),
            workspace: self.root.clone(),
        });
        if self.closed_tabs.len() > 30 { self.closed_tabs.remove(0); }
        self.active = match self.active {
            _ if self.documents.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.documents.len() - 1)),
            other => other,
        };
        self.refocus = true;
        cx.notify();
    }

    fn reopen_closed_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(tab) = self.closed_tabs.pop() {
            if tab.workspace != self.root { continue; }
            if let Some(path) = tab.path {
                // Deleted files are skipped; manually reopened files are already
                // present and should not prevent restoring the next closed tab.
                if !path.is_file() || self.documents.iter().any(|doc| doc.path.as_ref() == Some(&path)) {
                    continue;
                }
                self.open_path(path, window, cx);
            } else {
                self.add_document(None, tab.source.unwrap_or_default(), None, window, cx);
            }
            return;
        }
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
                "Discard and Close",
            ),
            CloseTarget::App => (
                "Automatic saving failed. Keep editing to preserve your changes, or discard and quit.",
                "Discard and Quit",
            ),
        };
        let receiver = window.prompt(
            PromptLevel::Warning,
            "You have unsaved changes",
            Some(detail),
            &["Keep Editing", discard],
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
        self.save_session(cx);
        if self.refocus {
            self.refocus = false;
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
        // Find belongs to the reading view of the tab it was opened in.
        let active_id = self.active.map(|i| self.documents[i].editor.entity_id());
        if let Some(find) = self.page_find.take_if(|f| self.mode == Mode::Edit || Some(f.editor_id) != active_id) {
            if let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == find.editor_id) {
                match &doc.large_reader {
                    Some(reader) => reader.update(cx, |reader, cx| reader.clear_highlight(cx)),
                    None => doc.preview.update(cx, |view, cx| view.clear_range_highlights(cx)),
                }
            }
        }
        if active_path != self.followed_path {
            self.followed_path = active_path.clone();
            if let Some(path) = &active_path {
                self.selected = Some(path.clone());
                self.marked.clear();
                self.reveal_in_tree(path, cx);
            }
        }
        let markdown = self.active.is_none_or(|i| self.documents[i].markdown);
        let mode = if markdown { self.mode } else { Mode::Edit };
        let active_doc = self.active.map(|i| self.documents[i].editor.entity_id());
        if active_doc != self.warmed_doc {
            self.warmed_doc = active_doc;
            self.warm_documents(cx);
        }
        self.run_pending_jump(window, cx);
        // A drag ended (dropped, or let go elsewhere): no drop target is shown.
        if !cx.has_active_drag() {
            self.dragging_tab = None;
            self.tab_drop = None;
            self.tree_drop = None;
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
            .overflow_x_scroll()
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                let slot = event.bounds.contains(&event.event.position)
                    .then(|| this.tab_insertion_slot(event.event.position.x));
                if this.tab_drop != slot {
                    this.tab_drop = slot;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, tab: &DraggedTab, window, cx| {
                let slot = this.tab_insertion_slot(window.mouse_position().x);
                // Removing the source shifts every later insertion boundary left.
                let to = if slot > tab.from { slot - 1 } else { slot };
                this.tab_drop = None;
                this.move_tab(tab.from, to, cx);
            }));
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
                    .debug_selector(move || format!("file-tab-{i}"))
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
                    // The whole strip handles drops, including between pills.
                    // Each tab only starts the drag and draws its insertion line.
                    .relative()
                    .on_drag(DraggedTab { from: i, title: document.title().into() }, {
                        let weak = cx.entity().downgrade();
                        move |tab, _, _, cx| {
                            let _ = weak.update(cx, |this, _| this.dragging_tab = Some(tab.from));
                            cx.new(|_| tab.clone())
                        }
                    })
                    .when(self.dragging_tab == Some(i), |tab| tab.opacity(0.4))
                    .child({
                        let slot = self.tab_drop;
                        let after = slot == Some(self.documents.len());
                        let shown = (slot == Some(i) || (after && i + 1 == self.documents.len()))
                            && self.dragging_tab.is_some_and(|from| {
                                slot != Some(from) && slot != Some(from + 1)
                            });
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .map(|slot| if after { slot.right(px(-3.)) } else { slot.left(px(-3.)) })
                            .child(ui::drop_line(("tab-drop", editor_id), shown, window, cx))
                    })
                    .when(active, |tab| tab.bg(theme.secondary))
                    .when(!active, |tab| {
                        tab.text_color(theme.muted_foreground)
                            .hover(|tab| tab.bg(theme.secondary.opacity(0.6)))
                    })
                    // Long names end in "…" at a fixed width; the full name is in
                    // the tooltip, and the unsaved dot stays outside the cut.
                    // A locked note says so on its tab.
                    .when(document.locked, |tab| {
                        tab.child(Icon::new(IconName::Lock).size_3().mr_1().text_color(theme.muted_foreground))
                    })
                    .child(
                        div()
                            .max_w(px(TAB_TITLE_WIDTH))
                            .min_w_0()
                            .truncate()
                            .child(document.title()),
                    )
                    .when(document.dirty(cx), |tab| tab.child("•"))
                    .tooltip({
                        let title = document.title();
                        move |window, cx| Tooltip::new(title.clone()).build(window, cx)
                    })
                    // Built only while shown: a hidden button per tab made every
                    // frame cost more with each open file.
                    .when(reveal > 0.001, |tab| tab.child(
                        h_flex()
                            .w(px(20. * reveal))
                            .h(px(20.))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .child(
                                ui::icon_button(("close-tab", editor_id), IconName::X, "Close file")
                                    .xsmall()
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
        // Selected rows: the focus colour while the sidebar has the keyboard
        // (as Finder does), a neutral tint otherwise. Strong enough to read
        // at a glance in both modes, never a solid block.
        let selected_bg = {
            let dark = cx.theme().mode.is_dark();
            if self.focus.is_focused(window) {
                cx.theme().ring.opacity(if dark { 0.30 } else { 0.18 })
            } else {
                cx.theme().foreground.opacity(if dark { 0.11 } else { 0.07 })
            }
        };
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
                                                ui::icon_button(("reveal-workspace", i), IconName::FolderOpen, reveal)
                                                    .xsmall()
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
                                                    ui::icon_button(("window-workspace", i), IconName::ExternalLink, "Open in new window")
                                                        .xsmall()
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
                                        ui::icon_button(("forget-workspace", i), IconName::X, "Remove from list")
                                            .xsmall()
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
                                .child("Open New Workspace…")
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
                                    ui::icon_button("open-workspace", IconName::Plus, "Open new workspace")
                                        .xsmall()
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.pick(true, w, cx)),
                                        ),
                                )
                            })
                            .when(self.root.is_some(), |row| {
                                row.child(
                                    ui::icon_button("refresh", IconName::RefreshCw, "Refresh folder")
                                        .xsmall()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let paths: Vec<_> =
                                                this.expanded.iter().cloned().collect();
                                            for path in paths {
                                                this.load_folder(path, cx);
                                            }
                                        })),
                                )
                                .child(
                                    ui::icon_button("new", IconName::Plus, "New File")
                                        .xsmall()
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
                    // Below the rows, a drop lands in the workspace itself.
                    .when_some(self.root.clone(), |list, root| {
                        let entry_root = root.clone();
                        let paths_root = root.clone();
                        let drop_entry_root = root.clone();
                        let drop_paths_root = root.clone();
                        list.on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DraggedEntry>, _, cx| {
                            if this.tree_drop_point != Some(e.event.position) {
                                this.hover_tree_drop(e.bounds, e.event.position, entry_root.clone(), cx);
                            }
                        }))
                        .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                            if this.tree_drop_point != Some(e.event.position) {
                                this.hover_tree_drop(e.bounds, e.event.position, paths_root.clone(), cx);
                            }
                        }))
                        .on_drop(cx.listener(move |this, e: &DraggedEntry, _, cx| {
                            this.move_entries(e.paths.clone(), drop_entry_root.clone(), cx)
                        }))
                        .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                            this.copy_into_folder(paths.paths().to_vec(), drop_paths_root.clone(), cx)
                        }))
                        .when(self.tree_drop.as_ref() == Some(&root), |list| {
                            list.bg(theme.ring.opacity(0.06))
                        })
                    })
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
                        let active = highlighted.as_ref() == Some(&path)
                            || (self.marked.len() > 1 && self.marked.contains(&path));
                        // Cut, waiting to be pasted somewhere.
                        let cut = self.copied_files.as_ref().is_some_and(|c| c.cut && c.paths.contains(&path));
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
                        // A drop on a folder lands in it; on a file, beside it.
                        let drop_dir = if entry.directory {
                            path.clone()
                        } else {
                            path.parent().map(PathBuf::from).unwrap_or_else(|| path.clone())
                        };
                        let drop_target = entry.directory && self.tree_drop.as_ref() == Some(&path);
                        h_flex()
                            .id(("file", i))
                            .relative()
                            .when(!pending, |row| {
                                row.on_drag(
                                    DraggedEntry {
                                        path: path.clone(),
                                        directory: entry.directory,
                                        paths: self.selection_with(&path),
                                    },
                                    |entry, _, _, cx| cx.new(|_| entry.clone()),
                                )
                            })
                            .on_drag_move(cx.listener({
                                let dir = drop_dir.clone();
                                move |this, e: &DragMoveEvent<DraggedEntry>, _, cx| {
                                    this.hover_tree_drop(e.bounds, e.event.position, dir.clone(), cx)
                                }
                            }))
                            .on_drag_move(cx.listener({
                                let dir = drop_dir.clone();
                                move |this, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                                    this.hover_tree_drop(e.bounds, e.event.position, dir.clone(), cx)
                                }
                            }))
                            .on_drop(cx.listener({
                                let dir = drop_dir.clone();
                                move |this, e: &DraggedEntry, _, cx| {
                                    this.move_entries(e.paths.clone(), dir.clone(), cx)
                                }
                            }))
                            .on_drop(cx.listener({
                                let dir = drop_dir.clone();
                                move |this, paths: &ExternalPaths, _, cx| {
                                    this.copy_into_folder(paths.paths().to_vec(), dir.clone(), cx)
                                }
                            }))
                            .when(drop_target, |row| row.bg(theme.ring.opacity(0.12)))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left_0()
                                    .child(ui::drop_line(("tree-drop", i), drop_target, window, cx)),
                            )
                            .h(px(TREE_ROW_HEIGHT))
                            .w_full()
                            .pl_1()
                            .pr_2()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            .when(active, |row| row.font_weight(FontWeight::MEDIUM))
                            .when(cut, |row| row.opacity(0.5))
                            .when(!active && !renaming, |row| row.text_color(muted))
                            .group("tree-row")
                            // Selection and hover start where the indent guides
                            // end, at the row's own chevron, and run to its end.
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(px(4. + TREE_INDENT * (depth + 1) as f32))
                                    .right_0()
                                    .rounded_md()
                                    .when(active || renaming, |fill| fill.bg(selected_bg))
                                    .when(!active && !renaming, |fill| {
                                        fill.group_hover("tree-row", |fill| {
                                            fill.bg(theme.foreground.opacity(0.05))
                                        })
                                    }),
                            )
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
                                // The tree takes focus, so Rename and Delete
                                // shortcuts act on the row just clicked.
                                let modifiers = event.modifiers();
                                if this.pick_row(&path, modifiers.shift, modifiers.secondary(cx)) {
                                    this.focus.focus(w, cx);
                                    cx.notify();
                                    return;
                                }
                                this.marked.clear();
                                this.selected = Some(path.clone());
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
                                        if this.selected.as_ref() != Some(&menu_path)
                                            && !(this.marked.len() > 1 && this.marked.contains(&menu_path))
                                        {
                                            this.marked.clear();
                                            this.selected = Some(menu_path.clone());
                                            cx.notify();
                                        }
                                    });
                                    let reveal = if cfg!(target_os = "macos") {
                                        "Show in Finder"
                                    } else {
                                        "Show in File Explorer"
                                    };
                                    menu.item(
                                        PopupMenuItem::new(reveal)
                                            .icon(IconName::FolderOpen)
                                            .on_click({
                                                let path = menu_path.clone();
                                                move |_, _, cx| cx.reveal_path(&path)
                                            }),
                                    )
                                    // As in Finder: everyday actions together, one
                                    // separator setting off the destructive one.
                                    .item(
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
                                                move |_, window, cx| {
                                                    let _ = weak.update(cx, |this, cx| {
                                                        this.delete_entries(this.selection_with(&path), window, cx)
                                                    });
                                                }
                                            }),
                                    )
                                }
                            })
                    }))
                    // The space below the rows: a click there selects nothing,
                    // so a paste or a new file goes in the workspace itself.
                    .child(
                        div()
                            .id("tree-blank")
                            .flex_grow(1.)
                            .min_h(px(TREE_ROW_HEIGHT))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.selected = None;
                                this.marked.clear();
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex().px_3().py_3().child(
                    ui::icon_button("settings", IconName::Settings, "Settings")
                        .small()
                        .on_click(|_, _, cx| settings::open(cx)),
                ),
            );

        let drop_tint = theme.ring.opacity(0.04);
        let mut body = h_flex()
            .id("document-body")
            .flex_1()
            .min_h_0()
            .w_full()
            // Files from Finder: images go into the note, text files open.
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(drop_tint))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_files(paths.paths().to_vec(), window, cx)
            }));
        if let Some(i) = self.active {
            let document = &self.documents[i];
            let mut editor_pane = None;
            let mut reading_pane = None;
            if mode != Mode::Read {
                editor_pane = Some(
                    div()
                        .id(("editor-pane", document.editor.entity_id()))
                        .when(document.markdown, |pane| {
                            markdown_commands(pane, &document.editor, document.locked)
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
                                // Locked: readable and selectable, not changeable.
                                .readonly(document.locked)
                                .size_full()
                                .font_family(theme.mono_font_family.clone())
                                .text_sm()
                                .when(document.markdown, |editor_view| {
                                    let editor = document.editor.clone();
                                    let checker = document.spellcheck.clone();
                                    editor_view.context_menu(move |menu, _, cx| {
                                        spellcheck::context_menu(menu, &editor, checker.as_ref(), cx)
                                    })
                                })
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
                    let owner = cx.entity().downgrade();
                    reader.update(cx, |reader, _| reader.set_controls(owner, document.editor.entity_id()));
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
                    let editor_id = document.editor.entity_id();
                    reading_pane = Some(
                        div()
                            .id(("preview-pane", document.editor.entity_id()))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            // A click in the reader moves the editor there too,
                            // as the editor's caret moves the reader. Seen on
                            // the way down, before the text view handles it.
                            .capture_any_mouse_up(cx.listener(move |_, event: &MouseUpEvent, window, cx| {
                                if event.button == MouseButton::Left {
                                    let position = event.position;
                                    // After the text view has settled its selection.
                                    cx.defer_in(window, move |this, _, cx| this.reader_clicked(editor_id, position, cx));
                                }
                            }))
                            .child(
                                div().size_full().child(
                                    // Spans the whole pane, like the editor, so its
                                    // scrollbar sits at the pane's right edge. The
                                    // padding is inside the scroll box: text starts
                                    // where the editor's does (Read and Edit swap in
                                    // place) and stops short of the overlaid bar.
                                    rich::decorate(TextView::new(&document.preview), cx.entity().downgrade(), document.editor.entity_id(), base.clone(), 0, 0)
                                        .map(|view| typography::style(view, cx))
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
            // Find, over the reading view it searches.
            if let Some(pane) = reading_pane.take() {
                let finding = self.page_find.as_ref().is_some_and(|f| f.editor_id == document.editor.entity_id());
                reading_pane = Some(match self.page_find_bar(cx).filter(|_| finding) {
                    Some(bar) => h_flex().relative().flex_1().min_w_0().h_full().child(pane).child(bar).into_any_element(),
                    None => pane,
                });
            }
            // Read and Edit have independent element paths. Only Split owns
            // resizable state, so switching modes never reuses panel zero for
            // a different kind of document view or its cached draw state.
            body = match (editor_pane, reading_pane) {
                (Some(editor), Some(reader)) => {
                    let (first, second) = if settings::reader_first(cx) {
                        (reader, editor)
                    } else {
                        (editor, reader)
                    };
                    body.child(
                        h_resizable(("document-split", document.editor.entity_id()))
                            .with_handle_appearance(std::rc::Rc::new(|handle, window, cx| {
                                Some(split_handle(handle, window, cx))
                            }))
                            .child(
                                resizable_panel()
                                    .size_range(px(180.)..Pixels::MAX)
                                    .child(first),
                            )
                            .child(
                                resizable_panel()
                                    .size_range(px(180.)..Pixels::MAX)
                                    .child(second),
                            ),
                    )
                }
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
                                ui::secondary_button("start-workspace", "Open Workspace")
                                    .icon(IconName::FolderOpen)
                                    .on_click(cx.listener(|this, _, w, cx| this.pick(true, w, cx))),
                            )
                            .child(
                                ui::secondary_button("start-open", "Open File")
                                    .icon(IconName::File)
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.pick(false, w, cx)),
                                    ),
                            )
                            .child(
                                ui::secondary_button("start-palette", "Command Palette")
                                    .icon(IconName::Search)
                                    .on_click(cx.listener(|this, _, w, cx| this.open_palette(w, cx))),
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
                        ui::icon_button(("mode", i), icon, label)
                            .small()
                            .disabled(!markdown || self.active.is_none())
                            .when(mode == value, |b| b.bg(theme.secondary))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_mode(value, window, cx)
                            }))
                    }),
                )
                // Reading preferences are global, with live size preview.
                .child({
                    let weak = cx.entity().downgrade();
                    div().debug_selector(|| "document-menu-trigger".into()).child(Popover::new("document-menu")
                        .overlay_closable(false)
                        // Compact: the kit's 12px, halved twice; items bring their own.
                        .p_1()
                        .trigger(ui::icon_button("document-menu-button", IconName::Ellipsis, "More")
                            .small().disabled(!markdown || self.active.is_none()))
                        .content(move |_, window, cx| {
                            let controls = window.use_keyed_state("document-reading-controls", cx, |window, cx| {
                                typography::ReadingControls::new(window, cx)
                            });
                            let weak = weak.clone();
                            let popover = cx.entity().downgrade();
                            let theme = cx.theme().clone();
                            let dismiss_controls = controls.clone();
                            let dismiss_popover = popover.clone();
                            let lock_weak = weak.clone();
                            let locked = weak.upgrade().is_some_and(|enso| {
                                let enso = enso.read(cx);
                                enso.active.is_some_and(|i| enso.documents[i].locked)
                            });
                            v_flex().debug_selector(|| "document-menu-panel".into()).w(px(240.))
                                // Room under the last row, matching the top.
                                .pb_1()
                                // Select's popup can extend beyond this surface.
                                // Wait for its outside-click handling before
                                // deciding whether the parent should close.
                                .on_mouse_down_out(move |_, window, cx| {
                                    let controls = dismiss_controls.clone();
                                    let popover = dismiss_popover.clone();
                                    window.defer(cx, move |window, cx| {
                                        if !controls.read(cx).font_dropdown_has_focus(window, cx) {
                                            let _ = popover.update(cx, |state, cx| state.dismiss(window, cx));
                                        }
                                    });
                                })
                                .child(controls)
                                .child(div().h(px(1.)).mx_1().my_1().bg(theme.border))
                                // The switch shows the state; the whole row flips it.
                                .child(h_flex().id("lock-page").px_2().h(px(28.)).gap_2()
                                    .items_center().rounded_md().cursor_pointer().text_sm()
                                    .hover(|row| row.bg(theme.secondary))
                                    .child(Icon::new(if locked { IconName::Lock } else { IconName::LockOpen })
                                        .size_4().text_color(theme.muted_foreground))
                                    .child(div().flex_1().child("Lock Page"))
                                    .child(gpui_kit::component::switch::Switch::new("lock-page-switch")
                                        .checked(locked).small().accessibility_label("Lock Page"))
                                    .on_click(move |_, _, cx| {
                                        let _ = lock_weak.update(cx, |this, cx| this.toggle_lock(cx));
                                    }))
                                // Last, always: the one item that leaves this menu.
                                .child(h_flex().id("export-note").px_2().h(px(28.)).gap_2()
                                    .items_center().rounded_md().cursor_pointer().text_sm()
                                    .hover(|row| row.bg(theme.secondary))
                                    .child(Icon::new(IconName::Share).size_4().text_color(theme.muted_foreground))
                                    .child(div().flex_1().child("Export…"))
                                    .on_click(move |_, window, cx| {
                                        let _ = popover.update(cx, |state, cx| state.dismiss(window, cx));
                                        let _ = weak.update(cx, |this, cx| this.open_export(window, cx));
                                    }))
                        }))
                }),
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
                                    ui::icon_button("toggle-sidebar", IconName::PanelLeft, toggle_label)
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
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
            .on_action(cx.listener(|this, _: &OpenCommandPalette, w, cx| this.open_palette(w, cx)))
            .on_action(cx.listener(|this, _: &SearchEverything, w, cx| this.toggle_content_search(w, cx)))
            .on_action(cx.listener(|this, _: &PasteFiles, w, cx| this.paste_files(w, cx)))
            .on_action(cx.listener(|this, _: &FindInPage, w, cx| this.open_page_find(w, cx)))
            .on_action(cx.listener(|this, _: &CopySelected, w, cx| this.copy_selected(false, w, cx)))
            .on_action(cx.listener(|this, _: &CutSelected, w, cx| this.copy_selected(true, w, cx)))
            .on_action(cx.listener(|this, action: &spellcheck::ReplaceSpelling, w, cx| this.replace_spelling(action, w, cx)))
            .on_action(cx.listener(|this, _: &ExportNote, w, cx| this.open_export(w, cx)))
            .on_action(cx.listener(|this, _: &ToggleLock, _, cx| this.toggle_lock(cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.cycle_tab(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| this.cycle_tab(-1, cx)))
            .on_action(cx.listener(|this, _: &CopyRichText, _, cx| {
                let images = this.export_choice.images;
                this.copy_rich_text(images, cx)
            }))
            .on_action(cx.listener(|this, _: &OpenSelectionInsertMenu, w, cx| {
                if !this.open_selection_insert_menu(w, cx) { cx.propagate(); }
            }))
            .on_action(cx.listener(|this, _: &OpenInsertMenu, w, cx| {
                if !this.open_insert_menu(w, cx) { cx.propagate(); }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| this.move_image_resize(event,cx)))
            .on_mouse_up(MouseButton::Left,cx.listener(|this,_,w,cx|this.finish_image_resize(w,cx)))
            .on_mouse_up_out(MouseButton::Left,cx.listener(|this,_,w,cx|this.finish_image_resize(w,cx)))
            .on_action(cx.listener(|this, _: &OpenFile, w, cx| this.pick(false, w, cx)))
            .on_action(cx.listener(|this, _: &OpenFolder, w, cx| this.pick(true, w, cx)))
            .on_action(cx.listener(|this, _: &NewFile, w, cx| this.new_file(w, cx)))
            .on_action(cx.listener(|this, _: &Save, _, cx| this.save(cx)))
            .on_action(cx.listener(|this, _: &ReopenClosedTab, w, cx| this.reopen_closed_tab(w, cx)))
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
                let picked = this.selection();
                if picked.is_empty() || !this.focus.is_focused(w) {
                    cx.propagate();
                } else {
                    this.delete_entries(picked, w, cx);
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
            .on_action(cx.listener(|this, _: &SwapSplitSides, _, cx| {
                if this.mode != Mode::Split { return; }
                if let Err(error) = settings::set_reader_first(!settings::reader_first(cx), cx) {
                    this.message(format!("Could not save split order: {error}"), true, cx);
                }
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
                                .child(h_flex().flex_1().min_h_0().min_w_0().relative().child(self.outline_rail(window,cx)).child(body.h_full().min_w_0()))
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
            .when_some(self.palette_overlay(window, cx), |view, overlay| view.child(overlay))
            .when_some(self.controls_overlay(window, cx), |view, overlay| view.child(overlay))
            .when_some(self.export_overlay(window, cx), |view, overlay| view.child(overlay))
            .when_some(self.lightbox_overlay(window, cx), |view, overlay| view.child(overlay))
            .when_some(self.toast_layer(window, cx), |view, layer| view.child(layer))
    }
}

fn bind_view_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissCommandPalette, Some("CommandPalette > Input")),
        KeyBinding::new("escape", DismissRichControls, Some("RichControls > Input")),
        KeyBinding::new("escape", DismissRichControls, Some("RichControls")),
        KeyBinding::new("escape", DismissExport, Some("ExportDialog")),
        KeyBinding::new("escape", DismissLightbox, Some("Lightbox")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-shift-t" } else { "ctrl-shift-t" }, ReopenClosedTab, Some("Enso")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-shift-p" } else { "ctrl-shift-p" }, OpenCommandPalette, Some("Enso")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-shift-s" } else { "ctrl-shift-s" }, SearchEverything, Some("Enso")),
        // At the window's depth only: in a Markdown editor the same keys are
        // Bold, which its deeper context binds first.
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-b" } else { "ctrl-b" }, ToggleSidebar, Some("Enso")),
        // Only acts while the sidebar has focus; text fields paste as usual.
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-v" } else { "ctrl-v" }, PasteFiles, Some("Enso")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-c" } else { "ctrl-c" }, CopySelected, Some("Enso")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-x" } else { "ctrl-x" }, CutSelected, Some("Enso")),
        // The editor binds the same keys closer in, for its own Find.
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-f" } else { "ctrl-f" }, FindInPage, Some("Enso")),
    ]);
    // Cmd on macOS, Ctrl elsewhere. On macOS, Ctrl+letter belongs to text
    // fields (Ctrl+E is end of line, Ctrl+A start of line, Ctrl+K delete to
    // the end), so enso binds none of them there. Bound at the input's depth
    // too, so a focused editor doesn't keep them.
    let primary = |mac: &'static str, other: &'static str| if cfg!(target_os = "macos") { mac } else { other };
    for context in ["Enso", "Enso > Input"] {
        cx.bind_keys([
            KeyBinding::new(primary("cmd-shift-\\", "ctrl-shift-\\"), SwapSplitSides, Some(context)),
            // Shifted punctuation is delivered as `|` on macOS, consuming
            // Shift. Accept both representations from platform keyboards.
            KeyBinding::new(primary("cmd-|", "ctrl-|"), SwapSplitSides, Some(context)),
            KeyBinding::new(primary("cmd-shift-|", "ctrl-shift-|"), SwapSplitSides, Some(context)),
            KeyBinding::new(primary("cmd-e", "ctrl-e"), ToggleView, Some(context)),
            KeyBinding::new(primary("cmd-w", "ctrl-w"), CloseFile, Some(context)),
            KeyBinding::new(primary("cmd-\\", "ctrl-\\"), ToggleSplit, Some(context)),
            // Ctrl+Tab on every platform: on macOS, Cmd+Tab belongs to the
            // system's app switcher, and browsers use Ctrl+Tab there too.
            KeyBinding::new("ctrl-tab", NextTab, Some(context)),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, Some(context)),
        ]);
    }
}

/// The split view's divider: the kit's line and pill, plus a button that swaps
/// the editor and reader sides. The button shows while the pointer is on the
/// divider or on the button itself; a press that moves still drags the divider.
fn split_handle(handle: &ResizeHandleContext, window: &mut Window, cx: &mut App) -> AnyElement {
    const SIZE: f32 = 24.;
    let line = resize_handle_appearance()(handle, window, cx);
    let shown = matches!(
        handle.state(),
        ResizeHandleState::Hovered | ResizeHandleState::Pressed
    );
    let theme = cx.theme();
    let hover_color = theme.foreground;
    let button = div()
        .id("split-swap")
        .absolute()
        .top(relative(0.5))
        .mt(px(-SIZE / 2.))
        .left(px(0.5 - SIZE / 2.))
        .size(px(SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .text_color(theme.muted_foreground)
        .cursor_pointer()
        .opacity(if shown { 1. } else { 0. })
        .hover(move |button| button.opacity(1.).text_color(hover_color))
        .child(Icon::new(IconName::ArrowLeftRight).size(px(13.)))
        .tooltip(|window, cx| Tooltip::new("Swap editor and reader").build(window, cx))
        .on_click(|_, _, cx| {
            let swapped = !settings::reader_first(cx);
            if let Err(error) = settings::set_reader_first(swapped, cx) {
                eprintln!("enso: could not save the split order: {error}");
            }
        });
    div()
        .flex_none()
        .relative()
        .w(px(1.))
        .h_full()
        .children(line)
        .child(button)
        .into_any_element()
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
        KeyBinding::new("/", OpenInsertMenu, context),
        KeyBinding::new(key!("/"), OpenSelectionInsertMenu, context),
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
fn markdown_commands(pane: Stateful<Div>, editor: &Entity<EditorState>, locked: bool) -> Stateful<Div> {
    use markdown_edit::{Block, Inline};
    // A command that always applies.
    macro_rules! command {
        ($pane:expr, $action:ty, $run:expr) => {{
            let editor = editor.clone();
            $pane.on_action(move |_: &$action, window, cx| {
                // A locked note takes no formatting.
                if locked {
                    return;
                }
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
                if locked || !apply_markdown(&editor, window, cx, $run) {
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
        // A locked note takes no edits, from keys or commands.
        if !state.is_editable() {
            return false;
        }
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
                    title: Some("enso".into()),
                    ..Default::default()
                }
            }),
            app_owns_titlebar_drag: CUSTOM_TITLEBAR,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(800.), px(500.))),
            window_background: window_backdrop(),
            // Linux docks and task switchers find Enso's icon and name through
            // the .desktop file of this id (scripts/install-linux.sh).
            app_id: Some("app.enso.markdown".into()),
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
        Err(error) => eprintln!("Unable to open an enso window: {error}"),
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

/// Look for a newer enso in the background (at most once a day, if allowed
/// in Settings) and say so in the front window. Never holds up startup;
/// a failed check stays silent.
fn check_for_updates(cx: &mut App) {
    if !settings::check_updates(cx) || !update::due() {
        return;
    }
    let found = cx.background_executor().spawn(async { update::check() });
    cx.spawn(async move |cx| {
        let Some(release) = found.await else { return };
        cx.update(|cx| {
            let active = cx.active_window();
            let views = open_views(cx);
            let Some((_, view)) = views
                .iter()
                .find(|(handle, _)| Some(*handle) == active)
                .or(views.first())
                .cloned()
            else {
                return;
            };
            let url = release.url;
            let toast = ui::toast::info("update", format!("enso {} is available", release.version))
                .action(IconName::Download, "Download", move |_, cx| cx.open_url(&url));
            view.update(cx, |app, cx| app.toast(toast, cx));
        });
    })
    .detach();
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
    let app = gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        // Web images in notes; GPUI's own client refuses every request.
        .with_http_client(std::sync::Arc::new(http::Client::new()));
    // The system can hand over files before the app has a context to open them
    // with; queue them until the run loop below picks them up.
    let (opened_tx, mut opened_rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = opened_tx.unbounded_send(urls);
    });
    app.run(move |cx| {
            gpui_kit::init(cx);
            typography::init(cx);
            app_icon::init();
            settings::init(cx);
            workspaces::init(cx);
            session::init(cx);
            lock::init(cx);
            cx.on_action(|_: &OpenSettings, cx| settings::open(cx));
            cx.bind_keys([
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," }, OpenSettings, None),
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
                KeyBinding::new(if cfg!(target_os = "macos") { "cmd-shift-n" } else { "ctrl-shift-n" }, NewFolder, Some("Enso")),
            ]);
            bind_view_keys(cx);
            bind_markdown_keys(cx);
            cx.set_menus(vec![
                Menu {
                    name: "enso".into(),
                    items: vec![
                        MenuItem::action("Settings…", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("Quit enso", Quit),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "File".into(),
                    items: vec![
                        MenuItem::action("Command Palette…", OpenCommandPalette),
                        MenuItem::action("Search Everything…", SearchEverything),
                        MenuItem::separator(),
                        MenuItem::action("Export…", ExportNote),
                        MenuItem::action("Copy as Rich Text", CopyRichText),
                        MenuItem::separator(),
                        MenuItem::action("Open File…", OpenFile),
                        MenuItem::action("Open Folder…", OpenFolder),
                        MenuItem::action("New File", NewFile),
                        MenuItem::action("New Folder", NewFolder),
                        MenuItem::action("Close File", CloseFile),
                        MenuItem::action("Reopen Closed Tab", ReopenClosedTab),
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
            check_for_updates(cx);
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
    fn reopen_closed_tabs_in_reverse_order(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-reopen-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let paths: Vec<_> = ["a.md", "b.md", "c.md"].iter().map(|name| root.join(name)).collect();
        for path in &paths { std::fs::write(path, "original").unwrap(); }
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                for path in &paths { app.add_document(Some(path.clone()), "original".into(), None, window, cx); }
                app
            })
        }).unwrap());
        draw(cx, handle);
        cx.update(|cx| view.update(cx, |app, cx| { app.close_document(1, cx); app.close_document(1, cx); }));
        std::fs::write(&paths[2], "changed on disk").unwrap();
        let key = if cfg!(target_os = "macos") { "cmd-shift-t" } else { "ctrl-shift-t" };
        draw(cx, handle);
        for expected in [2, 1] {
            cx.simulate_keystrokes(handle, key);
            cx.run_until_parked();
            draw(cx, handle);
            cx.update(|cx| {
                let app = view.read(cx);
                let doc = &app.documents[app.active.unwrap()];
                assert_eq!(doc.path.as_ref(), Some(&paths[expected]));
                if expected == 2 { assert_eq!(doc.saved.as_str(), "changed on disk"); }
            });
        }
        // No history is harmless and never creates a duplicate tab.
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert_eq!(cx.update(|cx| view.read(cx).documents.len()), 3);
        cx.update(|cx| view.update(cx, |app, cx| app.close_document(2, cx)));
        std::fs::remove_file(&paths[1]).unwrap();
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert_eq!(cx.update(|cx| view.read(cx).documents.len()), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn quick_reopens_bring_back_every_closed_tab(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-reopen-quick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let paths: Vec<_> = ["a.md", "b.md", "c.md", "d.md", "e.md"].iter().map(|name| root.join(name)).collect();
        for path in &paths { std::fs::write(path, "text").unwrap(); }
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        for path in &paths {
            let path = path.clone();
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |app, cx| app.open_path(path, window, cx))
            }).unwrap();
            cx.run_until_parked();
            draw(cx, handle);
        }
        for _ in 0..4 {
            cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
            cx.run_until_parked();
            draw(cx, handle);
        }
        assert_eq!(cx.update(|cx| view.read(cx).documents.len()), 1);
        // Typing in the tab that's left, then a press per closed tab: each
        // reopened tab takes the keyboard, so the next press still lands.
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |app, _| app.mode = Mode::Split);
            let editor = view.read(cx).documents[0].editor.clone();
            editor.focus_handle(cx).focus(window, cx);
        }).unwrap();
        draw(cx, handle);
        let key = if cfg!(target_os = "macos") { "cmd-shift-t" } else { "ctrl-shift-t" };
        for _ in 0..4 {
            cx.simulate_keystrokes(handle, key);
            cx.run_until_parked();
            draw(cx, handle);
        }
        cx.update(|cx| {
            let app = view.read(cx);
            let mut open: Vec<_> = app.documents.iter().filter_map(|d| d.path.clone()).collect();
            open.sort();
            assert_eq!(open, paths);
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn reopen_brings_back_every_closed_tab_in_turn(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-reopen-all-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let paths: Vec<_> = ["a.md", "b.md", "c.md"].iter().map(|name| root.join(name)).collect();
        for path in &paths { std::fs::write(path, "text").unwrap(); }
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Enso::new(window, cx))
        }).unwrap());
        for path in &paths {
            let path = path.clone();
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |app, cx| app.open_path(path, window, cx))
            }).unwrap();
            cx.run_until_parked();
            draw(cx, handle);
        }
        // Close all three from the keyboard, the last one open first.
        for _ in 0..3 {
            cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
            cx.run_until_parked();
            draw(cx, handle);
        }
        assert_eq!(cx.update(|cx| view.read(cx).documents.len()), 0);
        let key = if cfg!(target_os = "macos") { "cmd-shift-t" } else { "ctrl-shift-t" };
        for expected in [0, 1, 2] {
            cx.simulate_keystrokes(handle, key);
            cx.run_until_parked();
            draw(cx, handle);
            cx.update(|cx| {
                let app = view.read(cx);
                assert_eq!(app.documents.len(), expected + 1, "press {}", expected + 1);
            });
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn a_workspace_reopens_with_its_tabs_view_and_sidebar(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            // In memory only: no file, so the user's own sessions are untouched.
            cx.set_global(super::session::Sessions::default());
        });
        let root = std::env::temp_dir().join(format!("enso-session-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let deep = root.join("notes").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        let paths: Vec<_> = ["a.md", "b.md", "c.md"].iter().map(|name| root.join(name)).collect();
        for path in &paths { std::fs::write(path, "text").unwrap(); }
        std::fs::write(deep.join("d.md"), "text").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                // Opening a file deep inside expands its folders.
                app.add_document(Some(deep.join("d.md")), "text".into(), None, window, cx);
                app.close_document(0, cx);
                assert!(app.expanded.contains(&deep) && app.expanded.contains(&root.join("notes")));
                for path in &paths { app.add_document(Some(path.clone()), "text".into(), None, window, cx); }
                app.active = Some(1);
                app.mode = Mode::Split;
                app
            })
        }).unwrap());
        cx.run_until_parked();
        draw(cx, handle);
        // c is gone before the workspace is opened again: it's skipped.
        std::fs::remove_file(&paths[2]).unwrap();
        cx.update(|cx| { let _ = handle.update(cx, |_, window, _| window.remove_window()); });
        cx.run_until_parked();
        let (second, reopened) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        draw(cx, second);
        cx.update(|cx| {
            let app = reopened.read(cx);
            let tabs: Vec<_> = app.documents.iter().map(|d| d.path.clone().unwrap()).collect();
            assert_eq!(tabs, paths[..2]);
            assert_eq!(app.active, Some(1));
            assert!(app.mode == Mode::Split);
            // The folders expanded last time are expanded again.
            assert!(app.expanded.contains(&deep));
        });
        let _ = view;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn files_changed_on_disk_reload_or_ask(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-disk-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(&path, "first").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(path.clone()), "first".into(), None, window, cx);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let poll = |cx: &mut TestAppContext| {
            cx.executor().advance_clock(std::time::Duration::from_secs(2));
            cx.run_until_parked();
            draw(cx, handle);
        };
        let state = |cx: &mut TestAppContext| cx.update(|cx| {
            let doc = &view.read(cx).documents[0];
            (doc.editor.read(cx).value().to_string(), doc.dirty)
        });
        // Untouched in enso: the new text just appears, and isn't saved back.
        poll(cx);
        std::fs::write(&path, "changed elsewhere").unwrap();
        poll(cx);
        assert_eq!(state(cx), ("changed elsewhere".into(), false));
        poll(cx);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "changed elsewhere");
        // Unsaved edits here and a new file: enso asks, and keeps the edits.
        cx.update(|cx| { let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.documents[0].editor.update(cx, |e, cx| e.set_value("mine", window, cx)));
        }); });
        // A programmatic edit raises no change event; mark it as typing would.
        cx.update(|cx| view.update(cx, |app, _| app.documents[0].dirty = true));
        std::fs::write(&path, "theirs").unwrap();
        poll(cx);
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Keep My Version");
        poll(cx);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
        assert_eq!(state(cx), ("mine".into(), false));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn the_sidebar_follows_files_added_and_removed_outside_enso(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("open")).unwrap();
        std::fs::create_dir_all(root.join("closed")).unwrap();
        std::fs::write(root.join("first.md"), "").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.expanded.insert(root.join("open"));
                app.load_folder(root.join("open"), cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        let names = |cx: &mut TestAppContext, folder: &std::path::Path| -> Option<Vec<String>> {
            cx.update(|cx| {
                view.read(cx).folders.get(folder).map(|entries| {
                    let mut names: Vec<String> =
                        entries.iter().map(|e| e.path.file_name().unwrap().to_string_lossy().into()).collect();
                    names.sort();
                    names
                })
            })
        };
        let tick = |cx: &mut TestAppContext| {
            cx.executor().advance_clock(std::time::Duration::from_millis(1600));
            cx.run_until_parked();
        };
        // Added, renamed and deleted outside enso: the root and the expanded folder follow.
        std::fs::write(root.join("second.md"), "").unwrap();
        std::fs::write(root.join("open/inside.md"), "").unwrap();
        tick(cx);
        assert_eq!(names(cx, &root).unwrap(), ["closed", "first.md", "open", "second.md"]);
        assert_eq!(names(cx, &root.join("open")).unwrap(), ["inside.md"]);
        std::fs::rename(root.join("first.md"), root.join("renamed.md")).unwrap();
        std::fs::remove_file(root.join("open/inside.md")).unwrap();
        tick(cx);
        assert_eq!(names(cx, &root).unwrap(), ["closed", "open", "renamed.md", "second.md"]);
        assert_eq!(names(cx, &root.join("open")).unwrap(), Vec::<String>::new());
        // A collapsed folder isn't read until it's opened.
        std::fs::write(root.join("closed/later.md"), "").unwrap();
        tick(cx);
        assert!(names(cx, &root.join("closed")).is_none());
        // An expanded folder removed from disk leaves the sidebar.
        std::fs::remove_dir_all(root.join("open")).unwrap();
        tick(cx);
        assert_eq!(names(cx, &root).unwrap(), ["closed", "renamed.md", "second.md"]);
        cx.update(|cx| {
            let app = view.read(cx);
            assert!(!app.folders.contains_key(&root.join("open")));
            assert!(!app.expanded.contains(&root.join("open")));
        });
        draw(cx, handle);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn the_sidebar_selects_the_file_switched_to(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-follow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("deep")).unwrap();
        std::fs::write(root.join("one.md"), "one").unwrap();
        std::fs::write(root.join("deep/two.md"), "two").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.add_document(Some(root.join("deep/two.md")), "two".into(), None, window, cx);
                app.add_document(Some(root.join("one.md")), "one".into(), None, window, cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        draw(cx, handle);
        // Clicked in the sidebar, then the folder collapsed.
        cx.update(|cx| view.update(cx, |app, _| {
            app.selected = Some(root.join("one.md"));
            app.expanded.remove(&root.join("deep"));
        }));
        // Switching from the palette (an open tab): its row is selected and shown.
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.open_path(root.join("deep/two.md"), window, cx))
        });
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.selected.as_ref(), Some(&root.join("deep/two.md")));
            assert!(app.expanded.contains(&root.join("deep")));
        });
        // A folder clicked while the tab stays the same keeps the selection.
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("deep"))));
        draw(cx, handle);
        cx.update(|cx| assert_eq!(view.read(cx).selected.as_ref(), Some(&root.join("deep"))));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn the_sidebar_copies_and_pastes_files_only_while_it_has_focus(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-copy-paste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("folder")).unwrap();
        std::fs::write(root.join("note.md"), "note").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.add_document(Some(root.join("note.md")), "note".into(), None, window, cx);
                app.mode = Mode::Edit;
                app
            })
        }).unwrap());
        cx.run_until_parked();
        draw(cx, handle);
        let (copy, paste) = if cfg!(target_os = "macos") { ("cmd-c", "cmd-v") } else { ("ctrl-c", "ctrl-v") };
        let clipboard_files = |cx: &mut TestAppContext| -> Vec<PathBuf> {
            cx.update(|cx| cx.read_from_clipboard()).map_or(Vec::new(), |item| {
                item.entries().iter().filter_map(|e| match e {
                    gpui_kit::ClipboardEntry::ExternalPaths(p) => Some(p.paths().to_vec()),
                    _ => None,
                }).flatten().collect()
            })
        };
        // The note is selected in the sidebar, but the editor has focus: the
        // keys are the editor's, and no file is copied.
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("note.md"))));
        cx.simulate_keystrokes(handle, copy);
        cx.run_until_parked();
        assert!(clipboard_files(cx).is_empty());
        // Clicked in the sidebar (it has focus): the file is copied.
        cx.update_window(handle, |_, w, cx| view.update(cx, |app, cx| app.focus.focus(w, cx))).unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(handle, copy);
        cx.run_until_parked();
        assert_eq!(clipboard_files(cx), vec![root.join("note.md")]);
        // Pasted on a folder: copied into it; pasted again, a numbered copy.
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("folder"))));
        for _ in 0..2 {
            cx.simulate_keystrokes(handle, paste);
            cx.run_until_parked();
        }
        assert_eq!(std::fs::read_to_string(root.join("folder/note.md")).unwrap(), "note");
        assert!(root.join("folder/note 2.md").is_file());
        // A folder copies too, beside itself when pasted on itself.
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("folder"))));
        cx.simulate_keystrokes(handle, copy);
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("note.md"))));
        cx.simulate_keystrokes(handle, paste);
        cx.run_until_parked();
        assert!(root.join("folder 2/note 2.md").is_file());
        // Copied and pasted on itself: a copy inside it, made once (not a
        // copy that keeps copying itself).
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("folder"))));
        cx.simulate_keystrokes(handle, copy);
        cx.simulate_keystrokes(handle, paste);
        cx.run_until_parked();
        assert!(root.join("folder/folder/note.md").is_file());
        assert!(!root.join("folder/folder/folder").exists());
        std::fs::remove_dir_all(root.join("folder/folder")).unwrap();
        // Cut fades the row; pasting moves it (its tab follows), once.
        let cut = if cfg!(target_os = "macos") { "cmd-x" } else { "ctrl-x" };
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("note.md"))));
        cx.simulate_keystrokes(handle, cut);
        assert!(cx.update(|cx| view.read(cx).copied_files.as_ref().is_some_and(|c| c.cut)));
        assert!(root.join("note.md").is_file());
        cx.update(|cx| view.update(cx, |app, _| app.selected = Some(root.join("folder 2"))));
        cx.simulate_keystrokes(handle, paste);
        cx.run_until_parked();
        // "folder 2" already has a note.md and a note 2.md: both kept.
        assert!(!root.join("note.md").exists() && root.join("folder 2/note 3.md").is_file());
        assert_eq!(std::fs::read_to_string(root.join("folder 2/note.md")).unwrap(), "note");
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.documents[0].path.as_ref(), Some(&root.join("folder 2/note 3.md")));
            assert!(app.copied_files.is_none());
        });
        cx.simulate_keystrokes(handle, paste);
        cx.run_until_parked();
        assert!(!root.join("folder 2/note 4.md").exists());
        draw(cx, handle);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn several_sidebar_items_select_and_move_together(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-multi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        for name in ["a.md", "b.md", "c.md", "sub/d.md"] {
            std::fs::write(root.join(name), name).unwrap();
        }
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.expanded.insert(root.join("sub"));
                app.load_folder(root.join("sub"), cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| view.update(cx, |app, cx| app.focus.focus(w, cx))).unwrap();
        draw(cx, handle);
        let selection = |cx: &mut TestAppContext| cx.update(|cx| view.read(cx).selection());
        let p = |name: &str| root.join(name);
        // Shift-click from a.md to c.md picks the three; across folders it doesn't.
        cx.update(|cx| view.update(cx, |app, _| {
            app.selected = Some(p("a.md"));
            assert!(app.pick_row(&p("c.md"), true, false));
            assert!(!app.pick_row(&p("sub/d.md"), true, false));
        }));
        assert_eq!(selection(cx), vec![p("a.md"), p("b.md"), p("c.md")]);
        // Shift-click again shrinks the range from the same anchor.
        cx.update(|cx| view.update(cx, |app, _| assert!(app.pick_row(&p("b.md"), true, false))));
        assert_eq!(selection(cx), vec![p("a.md"), p("b.md")]);
        // Cmd/Ctrl-click adds a row from another folder, and takes one out.
        cx.update(|cx| view.update(cx, |app, _| {
            app.pick_row(&p("sub/d.md"), false, true);
            app.pick_row(&p("c.md"), false, true);
            app.pick_row(&p("c.md"), false, true);
        }));
        assert_eq!(selection(cx), vec![p("a.md"), p("b.md"), p("sub/d.md")]);
        // A row in the selection drags all of it; one outside drags alone.
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.selection_with(&p("b.md")).len(), 3);
            assert_eq!(app.selection_with(&p("c.md")), vec![p("c.md")]);
        });
        // Cut and pasted on a folder: all three move.
        let (cut, paste) = if cfg!(target_os = "macos") { ("cmd-x", "cmd-v") } else { ("ctrl-x", "ctrl-v") };
        cx.simulate_keystrokes(handle, cut);
        cx.update(|cx| view.update(cx, |app, _| { app.marked.clear(); app.selected = Some(p("target")); }));
        cx.simulate_keystrokes(handle, paste);
        cx.run_until_parked();
        for name in ["target/a.md", "target/b.md", "target/d.md"] {
            assert!(p(name).is_file(), "{name}");
        }
        assert!(!p("a.md").exists() && !p("sub/d.md").exists() && p("c.md").is_file());
        assert_eq!(selection(cx).len(), 3);
        // A folder and something inside it: the folder alone goes.
        cx.update(|cx| view.update(cx, |app, _| {
            app.marked = vec![p("target"), p("target/a.md")];
        }));
        assert_eq!(selection(cx), vec![p("target")]);
        // Deleting several asks once, for all of them.
        cx.update(|cx| view.update(cx, |app, _| app.marked = vec![p("c.md"), p("target/b.md")]));
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.delete_entries(app.selection(), window, cx))
        });
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(p("c.md").is_file() && p("target/b.md").is_file());
        draw(cx, handle);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn find_in_the_reader_counts_steps_and_closes(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); super::bind_markdown_keys(cx); });
        let source = "# Fruit\n\nAn **apple** pie, an apple tart.\n\nApples everywhere.\n";
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("fruit.md")), source.into(), None, window, cx);
                app.mode = Mode::Read;
                app
            })
        }).unwrap());
        for _ in 0..3 { draw(cx, handle); cx.run_until_parked(); }
        let find = if cfg!(target_os = "macos") { "cmd-f" } else { "ctrl-f" };
        cx.simulate_keystrokes(handle, find);
        draw(cx, handle);
        cx.simulate_input(handle, "apple");
        draw(cx, handle);
        let state = |cx: &mut TestAppContext| cx.update(|cx| {
            view.read(cx).page_find.as_ref().map(|f| (f.matches.len(), f.current))
        });
        // The rendered text is searched: "**apple**" counts, and so does "Apples".
        assert_eq!(state(cx), Some((3, 0)));
        cx.simulate_keystrokes(handle, "enter");
        assert_eq!(state(cx), Some((3, 1)));
        cx.simulate_keystrokes(handle, "shift-enter shift-enter");
        assert_eq!(state(cx), Some((3, 2)));
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        assert_eq!(state(cx), None);
        // In Edit, the keys are the editor's own Find.
        cx.update(|cx| view.update(cx, |app, _| app.mode = Mode::Edit));
        draw(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            let editor = view.read(cx).documents[0].editor.clone();
            editor.focus_handle(cx).focus(window, cx);
        }).unwrap();
        cx.simulate_keystrokes(handle, find);
        draw(cx, handle);
        assert_eq!(state(cx), None);
    }

    #[gpui_kit::test]
    fn find_in_a_large_note_paints_the_current_match(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let filler = "An ordinary line of a very long book.\n\n".repeat(8_000);
        let source = format!("{filler}Title: The Adventures of Sherlock Holmes\n\n{filler}");
        assert!(source.len() >= reader::LARGE_DOCUMENT_BYTES);
        let sections = reader::LargeReader::index(&source);
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("big.md")), source.clone(), Some(sections), window, cx);
                app.mode = Mode::Read;
                app
            })
        }).unwrap());
        for _ in 0..3 { draw(cx, handle); cx.run_until_parked(); }
        let find = if cfg!(target_os = "macos") { "cmd-f" } else { "ctrl-f" };
        cx.simulate_keystrokes(handle, find);
        draw(cx, handle);
        cx.simulate_input(handle, "sherlock");
        for _ in 0..6 { draw(cx, handle); cx.run_until_parked(); }
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.page_find.as_ref().unwrap().matches.len(), 1);
            let reader = app.documents[0].large_reader.as_ref().unwrap().read(cx);
            assert!(reader.highlight_painted());
        });
    }

    #[gpui_kit::test]
    fn a_search_result_selects_and_highlights_its_match(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-flash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("trip.md");
        let text = "# Trip\n\nPack the blue umbrella.\n";
        std::fs::write(&path, text).unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.mode = Mode::Split;
                app
            })
        }).unwrap());
        cx.run_until_parked();
        let offset = text.find("blue").unwrap();
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.open_path_at(path.clone(), offset, vec![offset..offset + 4], window, cx))
        });
        for _ in 0..6 { cx.run_until_parked(); draw(cx, handle); }
        cx.update(|cx| {
            let app = view.read(cx);
            let editor = app.documents[0].editor.read(cx);
            assert_eq!(editor.selected_range(), offset..offset + 4);
            assert!(app.flash_task.is_some());
        });
        // A few seconds later the highlight goes, the selection stays.
        cx.executor().advance_clock(std::time::Duration::from_secs(4));
        cx.run_until_parked();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn deleting_asks_first_and_cancel_keeps_the_file(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-confirm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("keep.md"), "").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        // Confirming is on unless turned off in Settings.
        cx.update(|cx| assert!(crate::settings::confirm_delete(cx)));
        let _ = handle.update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.delete_entry(root.join("keep.md"), window, cx))
        });
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(root.join("keep.md").is_file());
        draw(cx, handle);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn sidebar_moves_and_copies_keep_tabs_pointing_at_the_files(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let root = std::env::temp_dir().join(format!("enso-dnd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("folder")).unwrap();
        let outside = std::env::temp_dir().join(format!("enso-dnd-outside-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("note.md"), "note").unwrap();
        std::fs::write(outside.join("from finder.md"), "copied").unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.open_folder(root.clone(), cx);
                app.add_document(Some(root.join("note.md")), "note".into(), None, window, cx);
                app
            })
        }).unwrap());
        cx.run_until_parked();
        draw(cx, handle);
        // Dragging the open note into the folder moves the file; its tab follows.
        cx.update(|cx| view.update(cx, |app, cx| app.move_entry(root.join("note.md"), root.join("folder"), cx)));
        cx.run_until_parked();
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.documents[0].path.as_ref(), Some(&root.join("folder/note.md")));
            assert!(app.expanded.contains(&root.join("folder")));
        });
        assert!(root.join("folder/note.md").is_file() && !root.join("note.md").exists());
        // A Finder item dropped on the folder is copied in, twice with a number.
        for _ in 0..2 {
            cx.update(|cx| view.update(cx, |app, cx| {
                app.copy_into_folder(vec![outside.join("from finder.md")], root.join("folder"), cx)
            }));
            cx.run_until_parked();
        }
        assert!(root.join("folder/from finder.md").is_file());
        assert!(root.join("folder/from finder 2.md").is_file());
        assert!(outside.join("from finder.md").is_file());
        // Copies only land inside the workspace.
        cx.update(|cx| view.update(cx, |app, cx| app.copy_into_folder(vec![root.join("folder/note.md")], outside.clone(), cx)));
        cx.run_until_parked();
        assert!(!outside.join("note.md").exists());
        draw(cx, handle);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[gpui_kit::test]
    fn export_dialog_writes_the_chosen_format(cx: &mut TestAppContext) {
        use gpui_kit::BorrowAppContext;
        cx.update(|cx| {
            gpui_kit::init(cx); bind_view_keys(cx); super::settings::init(cx);
            cx.update_global::<super::settings::Preferences, _>(|p, _| p.reading.font = super::typography::ReadingFont::Poppins);
        });
        let root = std::env::temp_dir().join(format!("enso-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("pics")).unwrap();
        image::RgbImage::new(4, 4).save(root.join("pics/p.png")).unwrap();
        let note = root.join("Trip Notes.md");
        let text = "# Trip\n\n![photo](pics/p.png)\n";
        std::fs::write(&note, text).unwrap();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(note.clone()), text.into(), None, window, cx);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let run = |cx: &mut TestAppContext, format, images, name: &str| {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.open_export(window, cx);
                    let dialog = app.export_dialog.as_mut().unwrap();
                    dialog.choice.format = format;
                    dialog.choice.images = images;
                    cx.notify();
                })
            }).unwrap();
            draw(cx, handle);
            let target = root.join(name);
            cx.simulate_keystrokes(handle, "enter");
            cx.run_until_parked();
            cx.simulate_new_path_selection(|folder| {
                // Offered beside the note, under its name.
                assert_eq!(folder, root.as_path());
                Some(target)
            });
            cx.run_until_parked();
            draw(cx, handle);
        };
        run(cx, crate::export::Format::Html, false, "a.html");
        let html = std::fs::read_to_string(root.join("a/index.html")).unwrap();
        assert!(html.contains("<h1>Trip</h1>") && !html.contains("<img"));
        assert!(root.join("a/public/fonts/Poppins-Regular.ttf").is_file());
        assert!(std::fs::read_to_string(root.join("a/public/style.css")).unwrap().contains("font-family: \"Poppins\""));
        assert!(!root.join("a_files").exists());
        run(cx, crate::export::Format::Html, true, "b.html");
        assert!(root.join("b/public/images/p.png").is_file());
        run(cx, crate::export::Format::Pdf, true, "c.pdf");
        assert!(std::fs::read(root.join("c.pdf")).unwrap().starts_with(b"%PDF"));
        assert!(String::from_utf8_lossy(&std::fs::read(root.join("c.pdf")).unwrap()).contains("Poppins"));
        // The dialog offers the last choice again.
        assert!(cx.update(|cx| view.read(cx).export_choice.format) == crate::export::Format::Pdf);
        // Each export ends in one toast (its "Exporting…" one replaced).
        let toasts = cx.update(|cx| view.read(cx).toasts.iter().filter(|t| !t.leaving).count());
        assert_eq!(toasts, 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn toasts_replace_in_place_hide_themselves_and_stay_few(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Enso::new(window, cx))
        }).unwrap());
        let showing = |cx: &mut TestAppContext| cx.update(|cx| {
            view.read(cx).toasts.iter().filter(|t| !t.leaving).map(|t| t.toast.title.to_string()).collect::<Vec<_>>()
        });
        cx.update(|cx| view.update(cx, |app, cx| {
            app.toast(super::ui::toast::working("job", "Working…"), cx);
            app.toast(super::ui::toast::done("job", "Done", "detail"), cx);
        }));
        draw(cx, handle);
        // The spinner became the result: one toast, not two.
        assert_eq!(showing(cx), ["Done"]);
        cx.update(|cx| view.update(cx, |app, cx| {
            for n in 0..5 {
                app.toast(super::ui::toast::done(format!("n{n}"), format!("T{n}"), ""), cx);
            }
        }));
        draw(cx, handle);
        assert_eq!(showing(cx), ["T1", "T2", "T3", "T4"]);
        // A few seconds later they leave, and are gone once faded.
        cx.executor().advance_clock(std::time::Duration::from_secs(5));
        draw(cx, handle);
        assert!(showing(cx).is_empty());
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        draw(cx, handle);
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).toasts.is_empty()));
        // A working toast stays until it's finished; dismissing takes it away.
        cx.update(|cx| view.update(cx, |app, cx| app.toast(super::ui::toast::working("w", "Busy…"), cx)));
        cx.executor().advance_clock(std::time::Duration::from_secs(10));
        draw(cx, handle);
        assert_eq!(showing(cx), ["Busy…"]);
        cx.update(|cx| view.update(cx, |app, cx| app.dismiss_toast(&"w".into(), cx)));
        assert!(showing(cx).is_empty());
    }

    #[gpui_kit::test]
    fn moving_a_tab_keeps_the_same_document_active(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let paths: Vec<_> = ["a.md", "b.md", "c.md"].iter().map(PathBuf::from).collect();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                for path in &paths { app.add_document(Some(path.clone()), String::new(), None, window, cx); }
                app.active = Some(0);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let order = |cx: &mut TestAppContext| cx.update(|cx| {
            let app = view.read(cx);
            let names: Vec<_> = app.documents.iter().map(|d| d.path.clone().unwrap()).collect();
            (names, app.active)
        });
        // a moves to the end; it stays the active tab at its new place.
        cx.update(|cx| view.update(cx, |app, cx| app.move_tab(0, 2, cx)));
        assert_eq!(order(cx), (vec![paths[1].clone(), paths[2].clone(), paths[0].clone()], Some(2)));
        // c moves to the front, past the active a's neighbour; a stays active.
        cx.update(|cx| view.update(cx, |app, cx| app.move_tab(1, 0, cx)));
        assert_eq!(order(cx), (vec![paths[2].clone(), paths[1].clone(), paths[0].clone()], Some(2)));
        // Out of range is ignored.
        cx.update(|cx| view.update(cx, |app, cx| app.move_tab(0, 9, cx)));
        assert_eq!(order(cx).0.len(), 3);
        draw(cx, handle);
    }

    #[gpui_kit::test]
    fn shortcut_swaps_split_sides_from_editor_and_reader_and_remembers_it(cx: &mut TestAppContext) {
        use gpui_kit::BorrowAppContext;
        let path = std::env::temp_dir().join(format!("enso-split-shortcut-{}", std::process::id()));
        cx.update(|cx| {
            gpui_kit::init(cx); bind_view_keys(cx); super::settings::init(cx);
            cx.update_global::<super::settings::Preferences, _>(|p, _| {
                p.reader_first = false;
                p.split_order_path = Some(path.clone());
            });
        });
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(None, "# Heading\n\nBody".into(), None, window, cx);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let shortcut = if cfg!(target_os = "macos") { "cmd-shift-\\" } else { "ctrl-shift-\\" };
        cx.simulate_keystrokes(handle, shortcut);
        assert!(!cx.update(|cx| super::settings::reader_first(cx)), "Read mode does not swap");
        cx.update_window(handle, |_, w, cx| view.update(cx, |app, cx| app.set_mode(Mode::Split, w, cx))).unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(handle, shortcut);
        draw(cx, handle);
        assert!(cx.update(|cx| super::settings::reader_first(cx)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "reader-first");
        cx.update_window(handle, |_, w, cx| view.update(cx, |app, cx| app.focus.focus(w, cx))).unwrap();
        // macOS reports Shift+backslash as the shifted character `|`, with
        // Shift consumed, rather than as an unshifted key plus a modifier.
        let shifted_character = if cfg!(target_os = "macos") { "cmd-|" } else { "ctrl-|" };
        cx.simulate_keystrokes(handle, shifted_character);
        draw(cx, handle);
        assert!(!cx.update(|cx| super::settings::reader_first(cx)));
        assert!(cx.update(|cx| view.read(cx).mode == Mode::Split));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "editor-first");
        let retained_shift = if cfg!(target_os = "macos") { "cmd-shift-|" } else { "ctrl-shift-|" };
        cx.simulate_keystrokes(handle, retained_shift);
        draw(cx, handle);
        assert!(cx.update(|cx| super::settings::reader_first(cx)));
        assert!(cx.update(|cx| view.read(cx).mode == Mode::Split));
        // Without Shift, the same physical key still only toggles Split.
        let toggle = if cfg!(target_os = "macos") { "cmd-\\" } else { "ctrl-\\" };
        cx.simulate_keystrokes(handle, toggle);
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).mode == Mode::Read));
        cx.simulate_keystrokes(handle, toggle);
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).mode == Mode::Split));
        assert!(cx.update(|cx| super::settings::reader_first(cx)));
        std::fs::remove_file(path).unwrap();
    }

    #[gpui_kit::test]
    fn diagrams_copy_save_and_open_full_size(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let dir = std::env::temp_dir().join(format!("enso-media-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = "flowchart LR\n  A --> B";
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Enso::new(window, cx))
        }).unwrap());
        draw(cx, handle);
        // Copy: a PNG on the clipboard.
        cx.update(|cx| view.update(cx, |app, cx| app.copy_diagram(source, cx)));
        let copied = cx.update(|cx| cx.read_from_clipboard()).expect("clipboard");
        match copied.entries().first() {
            Some(gpui_kit::ClipboardEntry::Image(image)) => {
                assert_eq!(image.format(), gpui_kit::ImageFormat::Png);
                assert!(image.bytes().starts_with(b"\x89PNG"));
            }
            other => panic!("expected an image, got {other:?}"),
        }
        // Save as PNG and as SVG, wherever the dialog says.
        for (format, name, magic) in [
            (super::media::DiagramFormat::Png, "d.png", &b"\x89PNG"[..]),
            (super::media::DiagramFormat::Svg, "d.svg", &b"<svg"[..]),
        ] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |app, cx| app.save_diagram(source, format, Some(dir.clone()), window, cx))
            }).unwrap();
            cx.run_until_parked();
            let target = dir.join(name);
            cx.simulate_new_path_selection(|_| Some(target));
            cx.run_until_parked();
            assert!(std::fs::read(dir.join(name)).unwrap().starts_with(magic), "{name}");
        }
        // Full size opens, and Escape closes it.
        let drawn = crate::diagram::render(source, false).unwrap();
        cx.update_window(handle, |_, window, cx| view.update(cx, |app, cx| {
            app.open_lightbox(super::media::Lightbox::Diagram { svg: drawn.svg.clone(), width: drawn.width, height: drawn.height }, window, cx)
        })).unwrap();
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).lightbox.is_some()));
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).lightbox.is_none()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[gpui_kit::test]
    fn ctrl_shift_backslash_swaps_the_split_sides(cx: &mut TestAppContext) {
        // Cmd on macOS, Ctrl elsewhere; the shifted key also arrives as `|`
        // from some keyboards. On macOS, Ctrl keys are left to text fields.
        cx.update(|cx| {
            bind_view_keys(cx);
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            let swaps = |typed: &'static str| {
                let keystroke = gpui_kit::Keystroke::parse(typed).map_err(|e| e.to_string()).unwrap();
                keymap
                    .bindings_for_action(&super::SwapSplitSides)
                    .any(|binding| binding.match_keystrokes(&[keystroke.clone()]) == Some(false))
            };
            let typed = if cfg!(target_os = "macos") {
                ["cmd-shift-\\", "cmd-|"]
            } else {
                ["ctrl-shift-\\", "ctrl-|"]
            };
            for typed in typed {
                assert!(swaps(typed), "{typed} doesn't swap the split sides");
            }
            if cfg!(target_os = "macos") {
                assert!(!swaps("ctrl-shift-\\") && !swaps("ctrl-|"));
            }
        });
    }

    #[gpui_kit::test]
    fn macos_leaves_control_letters_to_text_fields(cx: &mut TestAppContext) {
        if !cfg!(target_os = "macos") {
            return;
        }
        cx.update(|cx| {
            bind_view_keys(cx);
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            // Only tab cycling uses Control on macOS.
            for binding in keymap.bindings() {
                let keys = binding.keystrokes();
                if let [key] = keys
                    && key.modifiers().control
                {
                    assert!(key.key() == "tab", "Ctrl+{} is bound on macOS", key.key());
                }
            }
        });
    }

    #[gpui_kit::test]
    fn ctrl_tab_cycles_through_the_tabs(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            super::bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                for name in ["a.md", "b.md", "c.md"] {
                    app.add_document(Some(PathBuf::from(name)), "text".into(), None, window, cx);
                }
                app.active = Some(0);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let active = |cx: &mut TestAppContext| cx.update(|cx| view.read(cx).active);
        cx.simulate_keystrokes(handle, "ctrl-tab");
        draw(cx, handle);
        assert_eq!(active(cx), Some(1));
        // Back past the first tab wraps around to the last.
        cx.simulate_keystrokes(handle, "ctrl-shift-tab ctrl-shift-tab");
        draw(cx, handle);
        assert_eq!(active(cx), Some(2));
        // In the editor too, without typing anything.
        cx.update(|cx| view.update(cx, |app, cx| { app.mode = Mode::Edit; cx.notify(); }));
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[2].editor.clone());
        cx.update_window(handle, |_, window, cx| editor.focus_handle(cx).focus(window, cx)).unwrap();
        cx.simulate_keystrokes(handle, "ctrl-tab");
        draw(cx, handle);
        assert_eq!(active(cx), Some(0));
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), "text");
    }

    #[gpui_kit::test]
    fn cmd_b_toggles_the_sidebar_except_in_the_markdown_editor(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            super::bind_markdown_keys(cx);
        });
        let text = "hello";
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("note.md")), text.into(), None, window, cx);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let key = if cfg!(target_os = "macos") { "cmd-b" } else { "ctrl-b" };
        let open = |cx: &mut TestAppContext| cx.update(|cx| view.read(cx).sidebar_open);
        // Reading: the keys collapse the sidebar, and bring it back.
        assert!(open(cx));
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert!(!open(cx));
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert!(open(cx));
        // Typing in the Markdown editor: they're Bold, and the sidebar stays.
        cx.update(|cx| view.update(cx, |app, cx| { app.mode = Mode::Edit; cx.notify(); }));
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| {
            editor.focus_handle(cx).focus(window, cx);
            editor.update(cx, |e, cx| e.set_selected_range(0..text.len(), cx));
        }).unwrap();
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert!(open(cx));
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), "**hello**");
    }

    #[gpui_kit::test]
    fn a_locked_page_takes_no_edits(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_view_keys(cx);
            super::bind_markdown_keys(cx);
        });
        let text = "- [ ] task\n\nhello";
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("locked.md")), text.into(), None, window, cx);
                app.mode = Mode::Edit;
                app
            })
        }).unwrap());
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| {
            editor.focus_handle(cx).focus(window, cx);
            editor.update(cx, |e, cx| e.set_selected_range(text.len()..text.len(), cx));
        }).unwrap();
        cx.update(|cx| view.update(cx, |app, cx| app.toggle_lock(cx)));
        draw(cx, handle);
        let value = |cx: &mut TestAppContext| cx.update(|cx| editor.read(cx).value().to_string());
        // Typing, a formatting shortcut, the reader's checkbox, the slash menu.
        cx.simulate_input(handle, " more");
        let bold = if cfg!(target_os = "macos") { "cmd-b" } else { "ctrl-b" };
        cx.simulate_keystrokes(handle, bold);
        let id = editor.entity_id();
        cx.update_window(handle, |_, window, cx| view.update(cx, |app, cx| app.toggle_read_task(id, 2, window, cx))).unwrap();
        cx.simulate_keystrokes(handle, "enter /");
        draw(cx, handle);
        assert_eq!(value(cx), text);
        assert!(cx.update(|cx| view.read(cx).palette.is_none()));
        // Unlocked, the same typing lands.
        cx.update(|cx| view.update(cx, |app, cx| app.toggle_lock(cx)));
        draw(cx, handle);
        cx.simulate_input(handle, "!");
        assert_eq!(value(cx), format!("{text}!"));
    }

    #[gpui_kit::test]
    fn clicking_the_reader_moves_the_editor_there(cx: &mut TestAppContext) {
        use gpui_kit::{Modifiers, VisualTestContext, point};
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let mut source = String::new();
        for n in 0..12 {
            source.push_str(&format!("## Section {n}\n\nSome words for section {n}.\n\n"));
        }
        source.push_str("- one\n- two\n- three\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn x() {}\n```\n");
        let text = source.clone();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("click.md")), text, None, window, cx);
                app
            })
        }).unwrap());
        cx.update_window(handle, |_, window, cx| view.update(cx, |app, cx| app.set_mode(Mode::Split, window, cx))).unwrap();
        for _ in 0..3 {
            draw(cx, handle);
        }
        // The reader lists one item per top-level block, as the source has.
        let blocks = super::top_level_blocks(&source);
        let (count, bounds) = cx.update(|cx| {
            let app = view.read(cx);
            let preview = app.documents[0].preview.read(cx);
            (preview.list_state().item_count(), preview.list_state().bounds_for_item(5))
        });
        assert_eq!(count, blocks.len());
        let bounds = bounds.expect("block 5 is on screen");
        let mut visual = VisualTestContext::from_window(handle, cx);
        visual.simulate_click(point(bounds.left() + px(4.), bounds.center().y), Modifiers::default());
        draw(cx, handle);
        let cursor = cx.update(|cx| view.read(cx).documents[0].editor.read(cx).cursor());
        assert_eq!(cursor, blocks[5].start, "caret at {:?}", &source[cursor..cursor + 20]);
    }

    #[gpui_kit::test]
    fn the_reader_lists_one_item_per_block_for_the_features_sample(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let source = include_str!("../Get Started.md").to_string()
            + "\n\n> quote\n>\n> > nested\n\n<table><tr><td>x</td></tr></table>\n\n<img src=\"a.png\" width=\"50%\" />\n\nA footnote.[^1]\n\n[^1]: Note.\n[gfm]: https://example.com\n\n$$\nE = mc^2\n$$\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\n- [x] done\n\n---\n";
        let text = source.clone();
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                app.add_document(Some(PathBuf::from("sample.md")), text, None, window, cx);
                app
            })
        }).unwrap());
        for _ in 0..3 {
            draw(cx, handle);
        }
        let count = cx.update(|cx| view.read(cx).documents[0].preview.read(cx).list_state().item_count());
        assert_eq!(count, super::top_level_blocks(&source).len());
    }

    #[gpui_kit::test]
    fn dragging_tabs_both_ways_including_gaps_and_end_of_strip(cx: &mut TestAppContext) {
        use gpui_kit::{Modifiers, MouseButton, VisualTestContext, point};
        cx.update(|cx| { gpui_kit::init(cx); bind_view_keys(cx); });
        let (handle, view) = cx.update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut app = Enso::new(window, cx);
                for name in ["a.md", "b.md", "c.md"] {
                    app.add_document(Some(PathBuf::from(name)), String::new(), None, window, cx);
                }
                app.active = Some(0);
                app
            })
        }).unwrap());
        draw(cx, handle);
        let mut visual = VisualTestContext::from_window(handle, cx);
        let order = |cx: &mut TestAppContext| cx.update(|cx| {
            let app = view.read(cx);
            (app.documents.iter().map(|doc| doc.title()).collect::<Vec<_>>(), app.active)
        });
        let mut drag = |from: usize, to: usize, after: bool, cx: &mut TestAppContext| {
            draw(cx, handle);
            let selectors = ["file-tab-0", "file-tab-1", "file-tab-2"];
            let start = visual.debug_bounds(selectors[from]).unwrap().center();
            let target = visual.debug_bounds(selectors[to]).unwrap();
            // Test insertion gaps. The strip starts at the first pill's edge.
            let finish = point(
                if after { target.right() + px(2.) }
                else if to == 0 { target.left() + px(2.) }
                else { target.left() - px(1.) },
                target.center().y,
            );
            visual.simulate_mouse_move(start, None, Modifiers::default());
            visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
            visual.simulate_mouse_move(start + point(px(5.), px(0.)), Some(MouseButton::Left), Modifiers::default());
            draw(cx, handle);
            visual.simulate_mouse_move(finish, Some(MouseButton::Left), Modifiers::default());
            draw(cx, handle);
            visual.simulate_mouse_up(finish, MouseButton::Left, Modifiers::default());
            draw(cx, handle);
        };
        drag(0, 2, true, cx);
        assert_eq!(order(cx), (vec!["b.md".to_string(), "c.md".to_string(), "a.md".to_string()], Some(2)));
        drag(2, 0, false, cx);
        assert_eq!(order(cx), (vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()], Some(0)));
        drag(0, 1, true, cx);
        assert_eq!(order(cx), (vec!["b.md".to_string(), "a.md".to_string(), "c.md".to_string()], Some(1)));
        drag(1, 0, false, cx);
        assert_eq!(order(cx), (vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()], Some(0)));
        // Dropping beside the source is a no-op; dropping outside cancels.
        drag(0, 0, true, cx);
        assert_eq!(order(cx), (vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()], Some(0)));
        let start = visual.debug_bounds("file-tab-0").unwrap().center();
        let outside = point(start.x, start.y + px(80.));
        visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(start + point(px(5.), px(0.)), Some(MouseButton::Left), Modifiers::default());
        draw(cx, handle);
        visual.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
        draw(cx, handle);
        visual.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
        draw(cx, handle);
        assert_eq!(order(cx), (vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()], Some(0)));
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
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().contains(&handle)));
        // With no tabs left, the next one closes the window.
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
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
            cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-e" } else { "ctrl-e" });
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
            cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-e" } else { "ctrl-e" });
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
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-\\" } else { "ctrl-\\" });
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
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-\\" } else { "ctrl-\\" });
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
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
        draw(cx, handle);
        cx.update(|cx| {
            assert_eq!(view.read(cx).documents.len(), 1);
            assert!(view.read(cx).error);
        });
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "external");
        // After resolving the conflict, the final shortcut closes the tab,
        // leaving the application window and an empty document list alive.
        std::fs::write(&paths[1], "edited-1").unwrap();
        cx.simulate_keystrokes(handle, if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" });
        draw(cx, handle);
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "my edit");
        cx.update(|cx| {
            assert!(view.read(cx).documents.is_empty());
            assert!(view.read(cx).active.is_none());
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
}
