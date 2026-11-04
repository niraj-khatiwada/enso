//! Searchable commands. Capturing the document and selection when opening keeps
//! formatting directed at the note, even while the search field owns focus.
use super::*;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use markdown_edit::{Block, Inline};
use std::ops::Range;

pub struct Palette {
    anchor: Option<Point<Pixels>>,
    replace_slash: bool,
    selection_bounds: Option<Bounds<Pixels>>,
    state: Entity<CommandState>,
    target: Option<(Entity<EditorState>, Range<usize>)>,
    previous_focus: Option<FocusHandle>,
    groups: Vec<Vec<Entry>>,
    table_input: Option<Entity<InputState>>,
    table_error: Option<String>,
    /// Set while the palette searches inside the workspace's notes.
    search: Option<ContentSearch>,
    _subscription: Option<Subscription>,
}

/// "Search everything": the palette's second mode, searching file contents.
struct ContentSearch {
    root: PathBuf,
    /// The workspace's text files, listed once when the mode opens.
    files: Option<Arc<Vec<PathBuf>>>,
    query: String,
    hits: Vec<search::Hit>,
    /// A search for `query` is running.
    searching: bool,
    /// Stops the search in flight when the query changes again.
    cancel: Arc<std::sync::atomic::AtomicBool>,
    _task: Option<Task<()>>,
}

const HISTORY_LIMIT: usize = 50;
#[derive(Default)]
struct History {
    keys: Vec<String>,
    path: Option<PathBuf>,
}
impl Global for History {}
fn ensure_history(cx: &mut App) {
    if cx.has_global::<History>() {
        return;
    }
    let path = dirs::config_dir().map(|base| files::app_folder(base).join("recent-commands"));
    let keys = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| decode_history(&text))
        .unwrap_or_default();
    cx.set_global(History { keys, path });
}
fn decode_history(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in text.lines() {
        for (name, key) in url::form_urlencoded::parse(line.as_bytes()) {
            if name == "command" && !keys.contains(&key.to_string()) && keys.len() < HISTORY_LIMIT {
                keys.push(key.into_owned());
            }
        }
    }
    keys
}
fn encode_history(keys: &[String]) -> String {
    keys.iter()
        .map(|key| {
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("command", key)
                .finish()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn remember(key: String, cx: &mut App) -> std::io::Result<()> {
    ensure_history(cx);
    cx.update_global::<History, _>(|history, _| {
        history.keys.retain(|old| old != &key);
        history.keys.insert(0, key);
        history.keys.truncate(HISTORY_LIMIT);
        if let Some(path) = &history.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, encode_history(&history.keys))?;
        }
        Ok(())
    })
}

#[derive(Clone)]
struct Entry {
    label: String,
    keywords: &'static [&'static str],
    command: Choice,
    shortcut: Option<&'static str>,
}
#[derive(Clone)]
enum Choice {
    Inline(Inline),
    Block(Block),
    Link,
    Image,
    Table,
    CodeBlock,
    Mermaid,
    Divider,
    Footnote,
    Highlight,
    Date,
    NewFile,
    NewFolder,
    Workspace(PathBuf),
    File(PathBuf),
    OpenWorkspace,
    Mode(Mode),
    Swap,
    Settings,
    Reveal,
    SearchEverything,
    Export,
    CopyRichText,
    ToggleLock,
    RenameFile,
    DeleteFile,
}
impl Choice {
    fn key(&self) -> String {
        match self {
            Self::Inline(Inline::Bold) => "bold".into(),
            Self::Inline(Inline::Italic) => "italic".into(),
            Self::Inline(Inline::Strikethrough) => "strikethrough".into(),
            Self::Inline(Inline::Code) => "inline-code".into(),
            Self::Block(Block::Heading(level)) => format!("heading-{level}"),
            Self::Block(Block::Bullet) => "bullet-list".into(),
            Self::Block(Block::Numbered) => "numbered-list".into(),
            Self::Block(Block::Task) => "checkbox".into(),
            Self::Block(Block::Quote) => "quote".into(),
            Self::Link => "link".into(),
            Self::Image => "image".into(),
            Self::Table => "table".into(),
            Self::CodeBlock => "code-block".into(),
            Self::Mermaid => "mermaid-diagram".into(),
            Self::Divider => "divider".into(),
            Self::Footnote => "footnote".into(),
            Self::Highlight => "highlight".into(),
            Self::Date => "date".into(),
            Self::NewFile => "new-file".into(),
            Self::NewFolder => "new-folder".into(),
            Self::Workspace(path) => format!("workspace:{}", path.display()),
            Self::File(path) => format!("file:{}", path.display()),
            Self::OpenWorkspace => "open-workspace".into(),
            Self::Mode(Mode::Read) => "read-view".into(),
            Self::Mode(Mode::Edit) => "edit-view".into(),
            Self::Mode(Mode::Split) => "split-view".into(),
            Self::Swap => "swap".into(),
            Self::Settings => "settings".into(),
            Self::Reveal => "reveal".into(),
            Self::SearchEverything => "search-everything".into(),
            Self::Export => "export".into(),
            Self::CopyRichText => "copy-rich-text".into(),
            Self::ToggleLock => "lock-page".into(),
            Self::RenameFile => "rename-file".into(),
            Self::DeleteFile => "delete-file".into(),
        }
    }

    /// Look up hints from the same Actions used by enso's keyboard bindings.
    fn action(&self) -> Option<Box<dyn Action>> {
        Some(match self {
            Self::Inline(Inline::Bold) => Box::new(Bold),
            Self::Inline(Inline::Italic) => Box::new(Italic),
            Self::Inline(Inline::Strikethrough) => Box::new(Strikethrough),
            Self::Inline(Inline::Code) => Box::new(InlineCode),
            Self::Link => Box::new(InsertLink),
            Self::Block(Block::Heading(0)) => Box::new(BodyText),
            Self::Block(Block::Heading(1)) => Box::new(Heading1),
            Self::Block(Block::Heading(2)) => Box::new(Heading2),
            Self::Block(Block::Heading(3)) => Box::new(Heading3),
            Self::Block(Block::Heading(4)) => Box::new(Heading4),
            Self::Block(Block::Heading(5)) => Box::new(Heading5),
            Self::Block(Block::Heading(6)) => Box::new(Heading6),
            Self::Block(Block::Bullet) => Box::new(BulletList),
            Self::Block(Block::Numbered) => Box::new(NumberedList),
            Self::Block(Block::Task) => Box::new(TaskList),
            Self::Block(Block::Quote) => Box::new(Quote),
            Self::CodeBlock => Box::new(CodeBlock),
            Self::NewFile => Box::new(NewFile),
            Self::NewFolder => Box::new(NewFolder),
            Self::OpenWorkspace => Box::new(OpenFolder),
            Self::Settings => Box::new(OpenSettings),
            Self::Mode(Mode::Split) => Box::new(ToggleSplit),
            Self::Swap => Box::new(SwapSplitSides),
            Self::SearchEverything => Box::new(SearchEverything),
            Self::Export => Box::new(ExportNote),
            Self::CopyRichText => Box::new(CopyRichText),
            Self::ToggleLock => Box::new(ToggleLock),
            _ => return None,
        })
    }
}

fn entry(
    label: impl Into<String>,
    keywords: &'static [&'static str],
    command: Choice,
    shortcut: Option<&'static str>,
) -> Entry {
    Entry {
        label: label.into(),
        keywords,
        command,
        shortcut,
    }
}

impl Enso {
    pub(super) fn open_selection_insert_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.open_insert_popover(false, window, cx)
    }

    pub(super) fn open_insert_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open_insert_popover(true, window, cx)
    }

    fn open_insert_popover(
        &mut self,
        replace_slash: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.active.map(|i| &self.documents[i]) else {
            return false;
        };
        if !doc.markdown || self.mode == Mode::Read || doc.locked {
            return false;
        }
        let editor = doc.editor.clone();
        let state = editor.read(cx);
        let selection = state.selected_range();
        let source = state.value();
        let start = source[..selection.start].rfind('\n').map_or(0, |i| i + 1);
        let end = source[selection.end..]
            .find('\n')
            .map_or(source.len(), |i| selection.end + i);
        if replace_slash && (!selection.is_empty() || !source[start..end].trim().is_empty()) {
            return false;
        }
        let selection_bounds = state.range_to_bounds(&selection);
        // Anchor to the first selected character, independently of which
        // end owns the caret (including backwards and multiline selections).
        let anchor = state
            .range_to_bounds(&(selection.start..selection.start))
            .map(|bounds| point(bounds.left(), bounds.bottom() + px(6.)))
            .or_else(|| {
                state
                    .cursor_layout()
                    .map(|(bounds, _)| point(bounds.left(), bounds.bottom() + px(6.)))
            })
            .unwrap_or(point(px(100.), px(120.)));
        if replace_slash {
            editor.update(cx, |state, cx| state.replace("/", window, cx));
        }
        self.open_palette(window, cx);
        if let Some(palette) = self.palette.as_mut() {
            palette.anchor = Some(anchor);
            palette.selection_bounds = selection_bounds;
            palette.replace_slash = replace_slash;
            palette.target = Some((
                editor,
                if replace_slash {
                    selection.start..selection.start + 1
                } else {
                    selection
                },
            ));
            // Recent insert commands only; app and formatting commands belong
            // to the full command palette.
            palette.groups[0].retain(|entry| is_insert(&entry.command));
            for group in &mut palette.groups[2..] {
                group.clear();
            }
        }
        true
    }

    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        self.closing_palette = None;
        ensure_history(cx);
        // A locked note offers nothing to insert or format.
        let target = self.active.and_then(|i| {
            let doc = &self.documents[i];
            (doc.markdown && self.mode != Mode::Read && !doc.locked)
                .then(|| (doc.editor.clone(), doc.editor.read(cx).selected_range()))
        });
        let mut insert = Vec::new();
        let mut format = Vec::new();
        if target.is_some() {
            insert = vec![
                entry("Link", &["url", "website"], Choice::Link, Some("K")),
                entry(
                    "Image…",
                    &["picture", "photo", "attachment"],
                    Choice::Image,
                    None,
                ),
                entry(
                    "Checkbox",
                    &["todo", "task", "checklist"],
                    Choice::Block(Block::Task),
                    Some("⇧L"),
                ),
                entry(
                    "Bullet List",
                    &["unordered", "points"],
                    Choice::Block(Block::Bullet),
                    Some("⇧8"),
                ),
                entry(
                    "Numbered List",
                    &["ordered", "steps"],
                    Choice::Block(Block::Numbered),
                    Some("⇧7"),
                ),
                entry("Table…", &["rows", "columns", "grid"], Choice::Table, None),
                entry(
                    "Code Block",
                    &["snippet", "fence"],
                    Choice::CodeBlock,
                    Some("⌥C"),
                ),
                entry(
                    "Mermaid Diagram",
                    &["diagram", "flowchart", "chart", "graph", "sequence"],
                    Choice::Mermaid,
                    None,
                ),
                entry(
                    "Quote",
                    &["blockquote", "citation"],
                    Choice::Block(Block::Quote),
                    Some("⇧."),
                ),
                entry(
                    "Divider",
                    &["line", "horizontal", "separator"],
                    Choice::Divider,
                    None,
                ),
                entry(
                    "Footnote",
                    &["reference", "citation"],
                    Choice::Footnote,
                    None,
                ),
                entry(
                    "Highlight",
                    &["mark", "yellow", "color", "background"],
                    Choice::Highlight,
                    None,
                ),
                entry("Today's Date", &["today", "calendar"], Choice::Date, None),
            ];
            format = vec![
                entry("Bold", &["strong"], Choice::Inline(Inline::Bold), Some("B")),
                entry(
                    "Italic",
                    &["emphasis"],
                    Choice::Inline(Inline::Italic),
                    Some("I"),
                ),
                entry(
                    "Strikethrough",
                    &["strike", "cross out"],
                    Choice::Inline(Inline::Strikethrough),
                    Some("⇧X"),
                ),
                entry(
                    "Inline Code",
                    &["monospace", "code"],
                    Choice::Inline(Inline::Code),
                    Some("⇧C"),
                ),
            ];
            for level in 1..=6 {
                format.push(entry(
                    format!("Heading {level}"),
                    &["title", "header"],
                    Choice::Block(Block::Heading(level)),
                    Some(match level {
                        1 => "1",
                        2 => "2",
                        3 => "3",
                        4 => "4",
                        5 => "5",
                        _ => "6",
                    }),
                ));
            }
            format.push(entry(
                "Body Text",
                &["paragraph", "normal"],
                Choice::Block(Block::Heading(0)),
                Some("0"),
            ));
        }
        let mut app = vec![
            entry("New File", &["note", "create"], Choice::NewFile, Some("N")),
            entry(
                "Open New Workspace…",
                &["folder", "switch workspace"],
                Choice::OpenWorkspace,
                Some("⇧O"),
            ),
            entry(
                "Settings",
                &["preferences", "appearance", "theme"],
                Choice::Settings,
                Some(","),
            ),
        ];
        if self.root.is_some() {
            app.insert(
                0,
                entry(
                    "Search Everything…",
                    &["find", "text", "content", "grep", "notes"],
                    Choice::SearchEverything,
                    Some("⇧S"),
                ),
            );
            app.push(entry(
                "New Folder",
                &["directory", "create"],
                Choice::NewFolder,
                Some("⇧N"),
            ));
        }
        if self.active.is_some_and(|i| self.documents[i].markdown) {
            app.extend([
                entry(
                    "Read View",
                    &["preview", "reading"],
                    Choice::Mode(Mode::Read),
                    None,
                ),
                entry(
                    "Edit View",
                    &["source", "writing"],
                    Choice::Mode(Mode::Edit),
                    None,
                ),
                entry(
                    "Split View",
                    &["preview", "side by side"],
                    Choice::Mode(Mode::Split),
                    Some("\\"),
                ),
                entry(
                    if self.active.is_some_and(|i| self.documents[i].locked) {
                        "Unlock Page"
                    } else {
                        "Lock Page"
                    },
                    &["lock", "unlock", "read only", "protect", "freeze", "edit"],
                    Choice::ToggleLock,
                    None,
                ),
                entry(
                    "Export…",
                    &["save", "pdf", "html", "print", "share", "document", "web"],
                    Choice::Export,
                    None,
                ),
                entry(
                    "Copy as Rich Text",
                    &["clipboard", "formatted", "email", "docs", "paste"],
                    Choice::CopyRichText,
                    None,
                ),
                entry(
                    "Swap Split Sides",
                    &["switch", "left", "right", "reader", "editor"],
                    Choice::Swap,
                    None,
                ),
            ]);
        }
        if self
            .active
            .and_then(|i| self.documents[i].path.as_ref())
            .is_some()
            || self.root.is_some()
        {
            app.push(entry(
                if cfg!(target_os = "macos") {
                    "Show in Finder"
                } else {
                    "Show in File Explorer"
                },
                &["reveal", "file manager", "folder"],
                Choice::Reveal,
                None,
            ));
        }
        // The open note's own file.
        if self.active.and_then(|i| self.documents[i].path.as_ref()).is_some() {
            app.push(entry("Rename File", &["name", "title", "file"], Choice::RenameFile, None));
            app.push(entry(
                "Delete File",
                &["remove", "trash", "bin", "file"],
                Choice::DeleteFile,
                None,
            ));
        }
        let workspaces = workspaces::recent(cx)
            .iter()
            .filter(|w| w.exists())
            .map(|w| {
                entry(
                    format!("Switch Workspace: {}", w.name()),
                    &["open", "folder", "workspace"],
                    Choice::Workspace(w.path.clone()),
                    None,
                )
            })
            .collect();
        let state = cx.new(|cx| CommandState::new(window, cx));
        let previous_focus = window.focused(cx);
        state.update(cx, |state, cx| state.focus(window, cx));
        if let Some((editor, _)) = &target {
            let focus = state.focus_handle(cx);
            editor.update(cx, |editor, cx| editor.set_selection_focus(Some(focus), cx));
        }
        let mut groups = vec![Vec::new(), insert, format, app, workspaces, Vec::new()];
        // Recently used commands lead the list in last-used order. Only commands
        // available in this context appear, and each appears once.
        let recent = cx.global::<History>().keys.clone();
        for key in recent {
            for group in &mut groups[1..] {
                if let Some(index) = group.iter().position(|entry| entry.command.key() == key) {
                    let entry = group.remove(index);
                    groups[0].push(entry);
                    break;
                }
            }
        }
        let palette_id = state.entity_id();
        self.palette = Some(Palette {
            anchor: None,
            replace_slash: false,
            selection_bounds: None,
            state,
            target,
            previous_focus,
            groups,
            table_input: None,
            table_error: None,
            search: None,
            _subscription: None,
        });
        if let Some(root) = self.root.clone() {
            let scan_root = root.clone();
            let task = cx
                .background_executor()
                .spawn(async move { workspace_files(&scan_root) });
            cx.spawn(async move |this, cx| {
                let paths = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.root.as_ref() != Some(&root) {
                        return;
                    }
                    let Some(palette) = this
                        .palette
                        .as_mut()
                        .filter(|p| p.state.entity_id() == palette_id && p.anchor.is_none())
                    else {
                        return;
                    };
                    let recent = &cx.global::<History>().keys;
                    let mut entries: Vec<_> = paths
                        .into_iter()
                        .map(|path| {
                            let label = path
                                .strip_prefix(&root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .into_owned();
                            entry(label, &["file", "open"], Choice::File(path), None)
                        })
                        .collect();
                    entries.sort_by_key(|e| {
                        recent
                            .iter()
                            .position(|key| *key == e.command.key())
                            .unwrap_or(usize::MAX)
                    });
                    palette.groups[5] = entries;
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(palette) = self.palette.take() {
            if let Some((editor, _)) = &palette.target {
                editor.update(cx, |editor, cx| editor.set_selection_focus(None, cx));
            }
            if let Some(focus) = &palette.previous_focus {
                focus.focus(window, cx);
            } else {
                self.focus.focus(window, cx);
            }
            // Keep the drawn surface until its exit fade has finished. Focus
            // returns immediately; closed commands cannot execute again.
            self.closing_palette = Some(palette);
            cx.notify();
        }
    }

    fn confirm_palette(
        &mut self,
        section: usize,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(choice) = self
            .palette
            .as_ref()
            .and_then(|p| p.groups.get(section)?.get(row))
            .map(|e| e.command.clone())
        else {
            return;
        };
        if matches!(choice, Choice::SearchEverything) {
            self.remember_command(&choice, cx);
            self.start_content_search(window, cx);
            return;
        }
        if matches!(choice, Choice::Table) {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value("3 x 3")
                    .placeholder("Rows x columns")
            });
            let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.insert_palette_table(window, cx);
                }
            });
            let focus = input.focus_handle(cx);
            if let Some((editor, _)) = self.palette.as_ref().and_then(|p| p.target.as_ref()) {
                editor.update(cx, |editor, cx| {
                    editor.set_selection_focus(Some(focus.clone()), cx)
                });
            }
            focus.focus(window, cx);
            let palette = self.palette.as_mut().unwrap();
            palette.table_input = Some(input);
            palette._subscription = Some(subscription);
            cx.notify();
            return;
        }
        if !matches!(choice, Choice::Image) {
            self.remember_command(&choice, cx);
        }
        let slash = self.palette.as_ref().is_some_and(|p| p.replace_slash);
        let target = self.palette.as_ref().and_then(|p| p.target.clone());
        self.close_palette(window, cx);
        if let Some((editor, selection)) = &target {
            // Restoring selection after search prevents commands from formatting
            // the query or inserting at a cursor moved by focus changes.
            editor.update(cx, |state, cx| {
                state.set_selected_range(selection.clone(), cx)
            });
        }
        match choice {
            Choice::NewFile => self.new_file(window, cx),
            Choice::NewFolder => self.new_folder(window, cx),
            Choice::OpenWorkspace => self.pick(true, window, cx),
            Choice::Workspace(path) => self.switch_workspace(path, false, window, cx),
            Choice::File(path) => {
                let in_workspace = self.root.as_ref().is_some_and(|root| {
                    std::fs::canonicalize(root)
                        .ok()
                        .zip(std::fs::canonicalize(&path).ok())
                        .is_some_and(|(root, file)| file.starts_with(root) && file.is_file())
                });
                if in_workspace {
                    self.open_path(path, window, cx);
                } else {
                    self.message(
                        "This file is no longer available in the workspace.",
                        true,
                        cx,
                    );
                }
            }
            Choice::Mode(mode) => self.set_mode(mode, window, cx),
            Choice::Swap => {
                if let Err(error) = settings::set_reader_first(!settings::reader_first(cx), cx) {
                    self.message(error.to_string(), true, cx);
                }
            }
            Choice::Settings => settings::open(cx),
            Choice::Export => self.open_export(window, cx),
            Choice::ToggleLock => self.toggle_lock(cx),
            Choice::RenameFile => {
                if let Some(path) = self.active.and_then(|i| self.documents[i].path.clone()) {
                    self.rename_file(path, window, cx);
                }
            }
            Choice::DeleteFile => {
                if let Some(path) = self.active.and_then(|i| self.documents[i].path.clone()) {
                    self.delete_entry(path, window, cx);
                }
            }
            Choice::CopyRichText => {
                let images = self.export_choice.images;
                self.copy_rich_text(images, cx)
            }
            Choice::Reveal => {
                if let Some(path) = self
                    .active
                    .and_then(|i| self.documents[i].path.as_ref())
                    .or(self.root.as_ref())
                {
                    cx.reveal_path(path);
                }
            }
            Choice::Image => {
                if let Some((editor, _)) = target {
                    self.pick_palette_image(editor, window, cx);
                }
            }
            choice => {
                if let Some((editor, _)) = target {
                    match choice {
                        Choice::Inline(kind) => {
                            apply_palette_markdown(&editor, slash, window, cx, |t, s| {
                                Some(markdown_edit::toggle_inline(t, s, kind))
                            });
                        }
                        Choice::Block(kind) => {
                            apply_palette_markdown(&editor, slash, window, cx, |t, s| {
                                Some(markdown_edit::toggle_block(t, s, kind))
                            });
                        }
                        Choice::Link => {
                            apply_palette_markdown(&editor, slash, window, cx, |t, s| {
                                Some(markdown_edit::link(t, s))
                            });
                        }
                        Choice::CodeBlock => {
                            apply_palette_markdown(&editor, slash, window, cx, |t, s| {
                                Some(markdown_edit::toggle_code_block(t, s))
                            });
                        }
                        Choice::Divider => insert(&editor, "\n\n---\n\n".into(), window, cx),
                        Choice::Mermaid => insert_mermaid(&editor, window, cx),
                        Choice::Highlight => self.open_highlight(editor.clone(), slash, window, cx),
                        Choice::Date => insert(
                            &editor,
                            chrono::Local::now().format("%Y-%m-%d").to_string(),
                            window,
                            cx,
                        ),
                        Choice::Footnote => {
                            let source = editor.read(cx).value().to_string();
                            let mut n = 1;
                            while source.contains(&format!("[^{n}]")) {
                                n += 1;
                            }
                            // Reference at the selection, definition at the end, one undo step.
                            let range = editor.read(cx).selected_range();
                            let mut suffix = source[range.end..].to_string();
                            suffix.push_str(&format!("\n\n[^{n}]: Footnote text\n"));
                            editor.update(cx, |state, cx| {
                                state.set_selected_range(range.start..source.len(), cx);
                                state.replace(
                                    format!(
                                        "{}[^{n}]{suffix}",
                                        if slash { "" } else { &source[range.clone()] }
                                    ),
                                    window,
                                    cx,
                                );
                                let start = state.value().len() - "Footnote text\n".len();
                                state.set_selected_range(start..start + "Footnote text".len(), cx);
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn pick_palette_image(
        &mut self,
        editor: Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = editor.read(cx).selected_range();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert image".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    let images: Vec<_> = paths
                        .into_iter()
                        .filter(|p| files::is_image(p))
                        .map(PastedImage::File)
                        .collect();
                    if images.is_empty() {
                        this.message("Choose an image file, such as PNG, JPEG or SVG.", true, cx);
                        return;
                    }
                    if this
                        .documents
                        .iter()
                        .any(|d| d.editor.entity_id() == editor.entity_id())
                    {
                        editor.update(cx, |state, cx| state.set_selected_range(selection, cx));
                        let before = editor.read(cx).value();
                        this.paste_images(images, editor.clone(), window, cx);
                        if editor.read(cx).value() != before {
                            this.remember_command(&Choice::Image, cx);
                        }
                        editor.focus_handle(cx).focus(window, cx);
                    }
                }
                Ok(Err(error)) => this.message(error.to_string(), true, cx),
                _ => {}
            });
        })
        .detach();
    }

    fn insert_palette_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(palette) = &self.palette else {
            return;
        };
        let Some(input) = &palette.table_input else {
            return;
        };
        let value = input.read(cx).value();
        let Some(table) = table_markdown(&value) else {
            self.palette.as_mut().unwrap().table_error =
                Some("Enter rows x columns, each between 1 and 20 (for example 3 x 4).".into());
            cx.notify();
            return;
        };
        let target = palette.target.clone();
        self.remember_command(&Choice::Table, cx);
        self.close_palette(window, cx);
        if let Some((editor, selection)) = target {
            editor.update(cx, |state, cx| state.set_selected_range(selection, cx));
            insert(&editor, table, window, cx);
        }
    }

    fn remember_command(&mut self, choice: &Choice, cx: &mut Context<Self>) {
        if let Err(error) = remember(choice.key(), cx) {
            self.message(format!("Could not save recent commands: {error}"), true, cx);
        }
    }

    pub(super) fn palette_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Sample even while closed to seed the initial value at zero. Keeping
        // the closing surface avoids removing the backdrop before it fades.
        let motion = cx.theme().motion_tokens();
        let opacity = transition(
            "command-palette-presence",
            if self.palette.is_some() {
                1.0_f32
            } else {
                0.0_f32
            },
            Transition::new(std::time::Duration::from_millis(180))
                .easing(motion.easing_move.clone()),
            window,
            cx,
        )
        .clamp(0., 1.);
        if self.palette.is_none() && opacity <= 0.001 {
            self.closing_palette = None;
            return None;
        }
        let palette = self.palette.as_ref().or(self.closing_palette.as_ref())?;
        let mut list_height = (window.viewport_size().height - px(160.)).clamp(px(100.), px(360.));
        let anchor = palette.anchor;
        let width = if anchor.is_some() { 280. } else { 520. };
        let left = anchor
            .map(|a| {
                a.x.min((window.viewport_size().width - px(width + 12.)).max(px(12.)))
                    .max(px(12.))
            })
            .unwrap_or((window.viewport_size().width - px(width)) / 2.);
        let top = if let Some(anchor) = anchor {
            let bounds = palette
                .selection_bounds
                .unwrap_or(Bounds::new(anchor, size(px(0.), px(0.))));
            let below = (window.viewport_size().height - bounds.bottom() - px(20.)).max(px(0.));
            let above = (bounds.top() - px(20.)).max(px(0.));
            // Prefer below the selected lines. If space is tight, flip above
            // and shorten the scrolling list rather than covering the text.
            let use_below = below >= px(240.) || below >= above;
            let available = if use_below { below } else { above };
            list_height = list_height.min((available - px(70.)).max(px(40.)));
            if use_below {
                bounds.bottom() + px(8.)
            } else {
                (bounds.top() - list_height - px(70.) - px(8.)).max(px(12.))
            }
        } else {
            px(64.)
        };
        let theme = cx.theme().clone();
        let content = if let Some(search) = &palette.search {
            self.content_search_view(&palette.state, search, list_height, cx)
        } else if let Some(input) = &palette.table_input {
            v_flex()
                .p_4()
                .gap_3()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Insert Table"),
                )
                .child(
                    div()
                        .text_sm()
                        .child("Data rows × columns (1–20). A header row is added."),
                )
                .child(Input::new(input))
                .when_some(palette.table_error.clone(), |d, e| {
                    d.child(div().text_sm().text_color(theme.danger).child(e))
                })
                .child(
                    h_flex().justify_end().child(
                        ui::primary_button("insert-table", "Insert Table").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.insert_palette_table(window, cx)
                            }),
                        ),
                    ),
                )
                .into_any_element()
        } else {
            let mut command = Command::new(&palette.state)
                .placeholder(if anchor.is_some() {
                    "Insert…"
                } else {
                    "Search commands and files…"
                })
                .max_h(list_height)
                .scroll_bounce(false)
                .rounded(ui::MODAL_RADIUS - px(1.))
                .bordered(false)
                .bg(gpui_kit::transparent_black());
            for (name, entries) in ["Recent", "Insert", "Format", "App", "Workspaces", "Files"]
                .into_iter()
                .zip(&palette.groups)
            {
                if entries.is_empty() {
                    continue;
                }
                // Keep section positions stable even with hidden Insert/Format.
                // on_confirm maps the visible group index below.
                let group = CommandGroup::new()
                    .label(name)
                    .items(entries.iter().map(|entry| {
                        let label = entry.label.clone();
                        let hint = entry.shortcut.map(|s| {
                            if cfg!(target_os = "macos") {
                                format!("⌘{s}")
                            } else {
                                format!("Ctrl+{}", s.replace('⇧', "Shift+").replace('⌥', "Alt+"))
                            }
                        });
                        let action = entry.command.action();
                        let focus = palette
                            .target
                            .as_ref()
                            .map(|(e, _)| e.focus_handle(cx))
                            .or_else(|| palette.previous_focus.clone());
                        CommandItem::new()
                            .label(label.clone())
                            .keywords(entry.keywords.iter().copied().chain(std::iter::once(name)))
                            .child(move |window, _| {
                                let binding = action.as_ref().and_then(|action| {
                                    focus.as_ref().and_then(|focus| {
                                        gpui_kit::component::kbd::Kbd::binding_for_action_in(
                                            action.as_ref(),
                                            focus,
                                            window,
                                        )
                                    })
                                });
                                h_flex()
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .gap_4()
                                    .child(label.clone())
                                    .map(|d| {
                                        if let Some(binding) = binding {
                                            d.child(binding)
                                        } else {
                                            d.when_some(hint.clone(), |d, hint| {
                                                d.child(div().text_xs().opacity(0.6).child(hint))
                                            })
                                        }
                                    })
                            })
                    }));
                command = command.group(group);
            }
            let sections: Vec<_> = palette
                .groups
                .iter()
                .enumerate()
                .filter(|(_, g)| !g.is_empty())
                .map(|(i, _)| i)
                .collect();
            let weak = cx.entity().downgrade();
            let cancel = weak.clone();
            command
                .on_confirm(move |index, window, cx| {
                    if let Some(&section) = sections.get(index.section) {
                        let _ = weak.update(cx, |this, cx| {
                            this.confirm_palette(section, index.row, window, cx)
                        });
                    }
                })
                .on_cancel(move |window, cx| {
                    let _ = cancel.update(cx, |this, cx| this.close_palette(window, cx));
                })
                .into_any_element()
        };
        Some(
            div()
                .key_context("CommandPalette")
                .occlude()
                .on_action(cx.listener(|this, _: &DismissCommandPalette, window, cx| {
                    this.close_palette(window, cx);
                }))
                .absolute()
                .inset_0()
                .bg(black().opacity(if anchor.is_some() {
                    0.
                } else {
                    ui::BACKDROP_OPACITY * opacity
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                )
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.close_palette(window, cx);
                    }
                }))
                .child(
                    div()
                        .absolute()
                        .top(
                            top - px(if anchor.is_some() {
                                0.
                            } else {
                                6. * (1. - opacity)
                            }),
                        )
                        .left(left)
                        .w(px(width))
                        .opacity(opacity)
                        .modal_surface(&theme)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        // Clip the command's own painted background and scroll
                        // list to the inside edge of the 1px border. Matching
                        // the inset radius avoids overlapping curved edges;
                        // leave the outer shadow unclipped.
                        .child(
                            div()
                                .rounded(ui::MODAL_RADIUS - px(1.))
                                .overflow_hidden()
                                .child(content),
                        ),
                )
                .into_any_element(),
        )
    }
}
// Reuse the explorer's hidden-file and symlink rules. All IO stays off
// the UI thread, including folders the user has not expanded yet.
impl Enso {
    /// Cmd/Ctrl+Shift+S: the palette straight in "Search everything", or
    /// closed again if it's already there.
    pub(super) fn toggle_content_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.as_ref().is_some_and(|p| p.search.is_some()) {
            self.close_palette(window, cx);
            return;
        }
        if self.root.is_none() {
            self.message("Open a folder to search inside its notes.", true, cx);
            return;
        }
        if self.palette.is_none() {
            self.open_palette(window, cx);
        }
        self.start_content_search(window, cx);
    }

    /// Switch the open palette to searching inside the workspace's notes.
    fn start_content_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |state, cx| state.focus(window, cx));
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        if let Some((editor, _)) = &palette.target {
            let focus = state.focus_handle(cx);
            editor.update(cx, |editor, cx| editor.set_selection_focus(Some(focus), cx));
        }
        palette.state = state;
        palette.anchor = None;
        palette.search = Some(ContentSearch {
            root: root.clone(),
            files: None,
            query: String::new(),
            hits: Vec::new(),
            searching: false,
            cancel: Default::default(),
            _task: None,
        });
        let task = cx.background_executor().spawn(async move {
            workspace_files(&root)
                .into_iter()
                .filter(|path| search::is_searchable(path))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let files = Arc::new(task.await);
            let _ = this.update(cx, |this, cx| {
                let Some(search) = this.palette.as_mut().and_then(|p| p.search.as_mut()) else {
                    return;
                };
                search.files = Some(files);
                // A query typed while the files were being listed runs now.
                let query = search.query.clone();
                if !query.is_empty() {
                    this.run_content_search(query, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn run_content_search(&mut self, query: String, cx: &mut Context<Self>) {
        let cache = self.search_cache.clone();
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        let state = palette.state.clone();
        let Some(search) = palette.search.as_mut() else {
            return;
        };
        search.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        search.query = query.clone();
        let Some(files) = search.files.clone() else {
            return; // Still listing files; it runs once they're in.
        };
        if query.trim().is_empty() {
            search.hits.clear();
            search.searching = false;
            search._task = None;
            cx.notify();
            return;
        }
        let cancel: Arc<std::sync::atomic::AtomicBool> = Default::default();
        search.cancel = cancel.clone();
        search.searching = true;
        let window = self.window_handle;
        search._task = Some(cx.spawn(async move |this, cx| {
            // Wait for a pause in typing.
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            let _ = window.update(cx, |_, window, cx| {
                state.update(cx, |s, cx| s.set_loading(true, window, cx))
            });
            let worker_cancel = cancel.clone();
            let hits = cx
                .background_executor()
                .spawn(async move { search::search(&files, &query, &cache, &worker_cancel) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let Some(search) = this.palette.as_mut().and_then(|p| p.search.as_mut()) {
                    search.hits = hits;
                    search.searching = false;
                }
                cx.notify();
            });
            let _ = window.update(cx, |_, window, cx| {
                state.update(cx, |s, cx| s.set_loading(false, window, cx))
            });
        }));
    }

    /// The results list: one group per file, one row per matching line with
    /// the matched words in bold.
    fn content_search_view(
        &self,
        state: &Entity<CommandState>,
        search: &ContentSearch,
        list_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let mut command = Command::new(state)
            .placeholder("Search in all notes…")
            .filterable(false)
            .max_h(list_height)
            .scroll_bounce(false)
            .rounded(ui::MODAL_RADIUS - px(1.))
            .bordered(false)
            .bg(gpui_kit::transparent_black());
        // Groups in the order their files' first hits appear.
        let mut groups: Vec<(PathBuf, Vec<usize>)> = Vec::new();
        for (i, hit) in search.hits.iter().enumerate() {
            match groups.last_mut() {
                Some((path, rows)) if *path == hit.path => rows.push(i),
                _ => groups.push((hit.path.clone(), vec![i])),
            }
        }
        for (path, rows) in &groups {
            let label = path
                .strip_prefix(&search.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let items = rows.iter().map(|&i| {
                let hit = search.hits[i].clone();
                let muted = theme.muted_foreground;
                CommandItem::new()
                    .label(hit.line.clone())
                    .child(move |_, _| {
                        let highlights = hit.ranges.iter().map(|r| {
                            (
                                r.clone(),
                                HighlightStyle {
                                    font_weight: Some(FontWeight::BOLD),
                                    ..Default::default()
                                },
                            )
                        });
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(StyledText::new(hit.line.clone()).with_highlights(highlights)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format!("line {}", hit.line_number)),
                            )
                    })
            });
            command = command.group(CommandGroup::new().label(label).items(items));
        }
        let empty = if search.query.trim().is_empty() {
            "Type to search inside every note in this workspace."
        } else if search.files.is_none() || search.searching {
            ""
        } else {
            "No notes contain that."
        };
        let weak = cx.entity().downgrade();
        let query_weak = weak.clone();
        let cancel = weak.clone();
        let rows: Vec<Vec<usize>> = groups.into_iter().map(|(_, rows)| rows).collect();
        command
            .empty(move |_, _, _| {
                div()
                    .p_4()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(empty)
            })
            .on_query(move |query, _, cx| {
                let query = query.to_string();
                let _ = query_weak.update(cx, |this, cx| this.run_content_search(query, cx));
            })
            .on_confirm(move |index, window, cx| {
                let Some(&hit) = rows.get(index.section).and_then(|r| r.get(index.row)) else {
                    return;
                };
                let _ = weak.update(cx, |this, cx| this.open_content_hit(hit, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |this, cx| this.close_palette(window, cx));
            })
            .into_any_element()
    }

    fn open_content_hit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hit) = self
            .palette
            .as_ref()
            .and_then(|p| p.search.as_ref())
            .and_then(|s| s.hits.get(index).cloned())
        else {
            return;
        };
        self.close_palette(window, cx);
        self.open_path_at(hit.path, hit.offset, window, cx);
    }
}

fn workspace_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut folders = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(folder) = folders.pop() {
        if let Ok(entries) = files::list_folder(&folder) {
            for entry in entries {
                if entry.directory {
                    folders.push(entry.path);
                } else {
                    files.push(entry.path);
                }
            }
        }
    }
    files.sort();
    files
}

fn is_insert(choice: &Choice) -> bool {
    !matches!(
        choice,
        Choice::Inline(_)
            | Choice::Block(Block::Heading(_))
            | Choice::NewFile
            | Choice::NewFolder
            | Choice::Workspace(_)
            | Choice::File(_)
            | Choice::OpenWorkspace
            | Choice::Mode(_)
            | Choice::Swap
            | Choice::Settings
            | Choice::Reveal
            | Choice::SearchEverything
            | Choice::Export
            | Choice::CopyRichText
            | Choice::ToggleLock
            | Choice::RenameFile
            | Choice::DeleteFile
    )
}

/// The diagram a new Mermaid block starts with.
const MERMAID_STARTER: &str = "flowchart LR\n  source[Markdown] --> parse --> doc[Document]";

/// A Mermaid block on its own lines at the selection, its starter diagram
/// selected so typing replaces it.
fn insert_mermaid(editor: &Entity<EditorState>, window: &mut Window, cx: &mut App) {
    let source = editor.read(cx).value().to_string();
    let range = editor.read(cx).selected_range();
    let before = &source[..range.start];
    let after = &source[range.end..];
    let lead = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let tail = if after.starts_with("\n\n") || after.is_empty() {
        ""
    } else if after.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let fence = "```mermaid\n";
    let text = format!("{lead}{fence}{MERMAID_STARTER}\n```{tail}");
    editor.update(cx, |state, cx| {
        state.replace(text, window, cx);
        let start = range.start + lead.len() + fence.len();
        state.set_selected_range(start..start + MERMAID_STARTER.len(), cx);
    });
}

fn apply_palette_markdown(
    editor: &Entity<EditorState>,
    slash: bool,
    window: &mut Window,
    cx: &mut App,
    command: impl FnOnce(&str, Range<usize>) -> Option<markdown_edit::Edit>,
) {
    apply_markdown(editor, window, cx, |text, selection| {
        if !slash {
            return command(text, selection);
        }
        let mut source = text.to_owned();
        source.replace_range(selection.clone(), "");
        let edit = command(&source, selection.start..selection.start)?;
        let start = edit.range.start.min(selection.start);
        let end = edit.range.end.max(selection.start);
        Some(markdown_edit::Edit {
            range: start..end + selection.len(),
            insert: format!(
                "{}{}{}",
                &source[start..edit.range.start],
                edit.insert,
                &source[edit.range.end..end]
            ),
            select: edit.select,
        })
    });
}

fn insert(editor: &Entity<EditorState>, text: String, window: &mut Window, cx: &mut App) {
    editor.update(cx, |state, cx| state.replace(text, window, cx));
}
fn table_markdown(value: &str) -> Option<String> {
    let dimensions: Vec<_> = value
        .split(|c: char| c == 'x' || c == '×' || c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if dimensions.len() != 2 {
        return None;
    }
    let rows: usize = dimensions[0].parse().ok()?;
    let cols: usize = dimensions[1].parse().ok()?;
    if !(1..=20).contains(&rows) || !(1..=20).contains(&cols) {
        return None;
    }
    let header = format!(
        "| {} |\n",
        (1..=cols)
            .map(|i| format!("Column {i}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let separator = format!("| {} |\n", vec!["---"; cols].join(" | "));
    let row = format!("| {} |\n", vec![" "; cols].join(" | "));
    Some(format!("\n\n{header}{separator}{}\n", row.repeat(rows)))
}
#[cfg(test)]
mod tests {
    #[test]
    fn history_preserves_paths_and_deduplicates() {
        let keys = vec![
            "workspace:/notes/日本語 & plans\n2026".to_owned(),
            "bold".to_owned(),
        ];
        let encoded = super::encode_history(&keys);
        assert_eq!(
            super::decode_history(&format!("{encoded}\n{encoded}")),
            keys
        );
    }
    #[test]
    fn table_dimensions_and_output() {
        let table = super::table_markdown("2 x 3").unwrap();
        assert!(table.contains("| Column 1 | Column 2 | Column 3 |"));
        assert_eq!(table.trim().lines().count(), 4);
        for bad in ["0 x 3", "2 x 21", "hello", "2 x 3 x 4"] {
            assert!(super::table_markdown(bad).is_none());
        }
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use crate::{Enso, Mode, bind_markdown_keys, bind_view_keys};
    use gpui_kit::{AnyWindowHandle, AppContext, Focusable, TestAppContext, WindowOptions};
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
    fn palette_scroll_stays_in_front_of_reading_view(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
        });
        let root = std::env::temp_dir().join(format!("enso-palette-scroll-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for n in 0..50 {
            std::fs::write(root.join(format!("fixture-{n:02}.md")), "note").unwrap();
        }
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.root = Some(root.clone());
                    app.add_document(None, "Long paragraph.\n\n".repeat(200), None, w, cx);
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.simulate_input(handle, "fixture");
        draw(cx, handle);
        let mut visual = gpui_kit::VisualTestContext::from_window(handle, cx);
        let center = cx
            .update_window(handle, |_, w, _| w.viewport_size().width / 2.)
            .unwrap();
        let before = cx.update(|cx| {
            view.read(cx).documents[0]
                .preview
                .read(cx)
                .list_state()
                .logical_scroll_top()
        });
        visual.simulate_mouse_move(
            gpui_kit::point(center, gpui_kit::px(200.)),
            None,
            gpui_kit::Modifiers::default(),
        );
        draw(cx, handle);
        let first = cx.update(|cx| {
            view.read(cx)
                .palette
                .as_ref()
                .unwrap()
                .state
                .read(cx)
                .selected_index()
                .unwrap()
                .row
        });
        visual.simulate_event(gpui_kit::ScrollEvent {
            position: gpui_kit::point(center, gpui_kit::px(200.)),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                gpui_kit::px(0.),
                gpui_kit::px(-500.),
            )),
            ..Default::default()
        });
        draw(cx, handle);
        visual.simulate_mouse_move(
            gpui_kit::point(center + gpui_kit::px(1.), gpui_kit::px(200.)),
            None,
            gpui_kit::Modifiers::default(),
        );
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            let after = app
                .palette
                .as_ref()
                .unwrap()
                .state
                .read(cx)
                .selected_index()
                .unwrap()
                .row;
            assert!(
                after > first,
                "wheel should reveal later file rows: {first} -> {after}"
            );
            assert_eq!(
                app.documents[0]
                    .preview
                    .read(cx)
                    .list_state()
                    .logical_scroll_top(),
                before,
                "background reader must stay still"
            );
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn search_everything_finds_lines_and_opens_at_the_match(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let root = std::env::temp_dir().join(format!("enso-palette-grep-{}", std::process::id()));
        std::fs::create_dir_all(root.join("notes")).unwrap();
        let path = root.join("notes/trip.md");
        let text = "# Trip\n\nPack the blue umbrella.\n";
        std::fs::write(&path, text).unwrap();
        std::fs::write(root.join("other.md"), "nothing here\n").unwrap();
        std::fs::write(root.join("photo.png"), "blue umbrella").unwrap();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.root = Some(root.clone());
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let key = if cfg!(target_os = "macos") { "cmd-shift-s" } else { "ctrl-shift-s" };
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        cx.simulate_input(handle, "umbrella blue");
        cx.executor().advance_clock(std::time::Duration::from_millis(500));
        draw(cx, handle);
        cx.update(|cx| {
            let search = view.read(cx).palette.as_ref().unwrap().search.as_ref().unwrap();
            // Only notes are searched, not other files.
            assert_eq!(search.hits.len(), 1);
            assert_eq!(search.hits[0].path, path);
            assert_eq!(search.hits[0].line, "Pack the blue umbrella.");
        });
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            assert!(app.palette.is_none());
            let doc = &app.documents[app.active.unwrap()];
            assert_eq!(doc.path.as_ref(), Some(&path));
            let cursor = doc.editor.read(cx).cursor();
            assert_eq!(&text[cursor..cursor + 4], "blue");
        });
        // The shortcut closes the search again.
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).palette.is_none()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn workspace_file_search_opens_and_reuses_tabs(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let root = std::env::temp_dir().join(format!("enso-palette-files-{}", std::process::id()));
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        let path = root.join("notes/unique-note.md");
        std::fs::write(&path, "# My note").unwrap();
        std::fs::write(root.join(".hidden/secret.md"), "hidden").unwrap();
        std::fs::write(root.join(".secret.md"), "hidden").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::temp_dir(), root.join("outside-link")).unwrap();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.root = Some(root.clone());
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let key = if cfg!(target_os = "macos") {
            "cmd-shift-p"
        } else {
            "ctrl-shift-p"
        };
        for _ in 0..2 {
            cx.simulate_keystrokes(handle, key);
            draw(cx, handle);
            cx.update(|cx| {
                let app = view.read(cx);
                let results = &app.palette.as_ref().unwrap().groups[5];
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].label, "notes/unique-note.md");
            });
            cx.simulate_input(handle, "unique-note");
            draw(cx, handle);
            cx.simulate_keystrokes(handle, "enter");
            draw(cx, handle);
            cx.update(|cx| {
                let app = view.read(cx);
                assert_eq!(app.documents.len(), 1);
                assert_eq!(
                    app.documents[app.active.unwrap()].path.as_ref(),
                    Some(&path)
                );
            });
        }
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.set_mode(Mode::Edit, window, cx);
            })
        })
        .unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-/"
            } else {
                "ctrl-/"
            },
        );
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            let palette = app.palette.as_ref().unwrap();
            assert!(palette.groups[5].is_empty());
            assert!(
                palette.groups[0]
                    .iter()
                    .all(|entry| !matches!(entry.command, super::Choice::File(_)))
            );
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn control_slash_formats_selected_text(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(
                        Some(PathBuf::from("selection.md")),
                        "hello world".into(),
                        None,
                        window,
                        cx,
                    );
                    app.mode = Mode::Split;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        for (query, expected) in [
            ("highlight", "<mark color=\"#fef08a\">hello</mark> world"),
            ("link", "[hello](url) world"),
        ] {
            cx.update_window(handle, |_, window, cx| {
                editor.update(cx, |state, cx| {
                    state.set_selected_range(0..state.value().len(), cx);
                    state.replace("hello world", window, cx);
                    state.set_selected_range(0..5, cx);
                });
                editor.focus_handle(cx).focus(window, cx);
            })
            .unwrap();
            draw(cx, handle);
            cx.simulate_keystrokes(
                handle,
                if cfg!(target_os = "macos") {
                    "cmd-/"
                } else {
                    "ctrl-/"
                },
            );
            draw(cx, handle);
            cx.update(|cx| {
                let app = view.read(cx);
                let palette = app.palette.as_ref().unwrap();
                assert!(palette.anchor.is_some());
                assert!(!palette.replace_slash);
                assert_eq!(editor.read(cx).value().as_str(), "hello world");
            });
            cx.update_window(handle, |_, window, cx| {
                assert!(editor.read(cx).has_selection_focus(window, cx));
            })
            .unwrap();
            cx.simulate_input(handle, query);
            draw(cx, handle);
            cx.simulate_keystrokes(handle, "enter");
            draw(cx, handle);
            if query == "highlight" {
                cx.update_window(handle, |_, w, cx| {
                    view.update(cx, |app, cx| app.save_controls(w, cx))
                })
                .unwrap();
                draw(cx, handle);
            }
            assert_eq!(
                cx.update(|cx| editor.read(cx).value().to_string()),
                expected
            );
        }
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |state, cx| state.set_selected_range(1..6, cx));
            editor.focus_handle(cx).focus(window, cx);
        })
        .unwrap();
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-/"
            } else {
                "ctrl-/"
            },
        );
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        cx.update(|cx| {
            assert_eq!(editor.read(cx).selected_range(), 1..6);
            assert_eq!(editor.read(cx).value().as_str(), "[hello](url) world");
        });
    }

    #[gpui_kit::test]
    fn rename_file_from_the_palette_starts_the_sidebar_rename(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
        });
        let root = std::env::temp_dir().join(format!("enso-palette-rename-{}", std::process::id()));
        std::fs::create_dir_all(root.join("notes")).unwrap();
        let path = root.join("notes/a.md");
        std::fs::write(&path, "a").unwrap();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.root = Some(root.clone());
                    app.sidebar_open = false;
                    app.add_document(Some(path.clone()), "a".into(), None, window, cx);
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let key = if cfg!(target_os = "macos") { "cmd-shift-p" } else { "ctrl-shift-p" };
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        cx.simulate_input(handle, "rename file");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.renaming.as_ref(), Some(&path));
            assert!(app.sidebar_open);
            assert!(app.expanded.contains(&root.join("notes")));
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn mermaid_diagram_inserts_from_slash_and_the_palette(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(Some(PathBuf::from("diagram.md")), "".into(), None, window, cx);
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| editor.focus_handle(cx).focus(window, cx)).unwrap();
        draw(cx, handle);
        let block = format!("```mermaid\n{}\n```", super::MERMAID_STARTER);
        // From the slash menu on an empty line: the slash becomes the block,
        // and the starter diagram is selected, ready to be typed over.
        cx.simulate_keystrokes(handle, "/");
        draw(cx, handle);
        cx.simulate_input(handle, "mermaid");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        cx.update(|cx| {
            let state = editor.read(cx);
            assert_eq!(state.value().as_ref(), block);
            let selected = &state.value()[state.selected_range()];
            assert_eq!(selected, super::MERMAID_STARTER);
        });
        // From the command palette after text: on its own lines below it.
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |state, cx| {
                state.set_value("Intro", window, cx);
                state.set_selected_range(5..5, cx);
            })
        })
        .unwrap();
        let key = if cfg!(target_os = "macos") { "cmd-shift-p" } else { "ctrl-shift-p" };
        cx.simulate_keystrokes(handle, key);
        draw(cx, handle);
        cx.simulate_input(handle, "mermaid diagram");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        let text = cx.update(|cx| editor.read(cx).value().to_string());
        assert_eq!(text, format!("Intro\n\n{block}"));
    }

    #[gpui_kit::test]
    fn slash_inserts_and_recent_commands_lead(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(Some(PathBuf::from("slash.md")), "".into(), None, window, cx);
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| {
            editor.focus_handle(cx).focus(window, cx)
        })
        .unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "/");
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).palette.as_ref().unwrap().anchor.is_some()));
        cx.simulate_input(handle, "checkbox");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        let text = cx.update(|cx| editor.read(cx).value().to_string());
        assert!(text.starts_with("- [ ] "), "{text:?}");
        assert!(!text.contains('/'));
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.update(|cx| {
            let app = view.read(cx);
            assert_eq!(app.palette.as_ref().unwrap().groups[0][0].label, "Checkbox");
        });
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        // A slash within text falls through to the editor normally.
        cx.simulate_keystrokes(handle, "/");
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).palette.is_none()));
        assert!(cx.update(|cx| editor.read(cx).value().contains('/')));
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |state, cx| {
                state.set_selected_range(0..state.value().len(), cx);
                state.replace("", window, cx);
            });
        })
        .unwrap();
        cx.simulate_keystrokes(handle, "/");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        assert_eq!(cx.update(|cx| editor.read(cx).value().to_string()), "/");
    }

    #[gpui_kit::test]
    fn palette_formats_original_selection_and_restores_focus(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
            bind_markdown_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(
                        Some(PathBuf::from("palette.md")),
                        "hello world".into(),
                        None,
                        window,
                        cx,
                    );
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |state, cx| state.set_selected_range(0..5, cx));
            editor.focus_handle(cx).focus(window, cx);
        })
        .unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).palette.is_some()));
        cx.simulate_input(handle, "bold");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            assert!(view.read(cx).palette.is_none());
            assert_eq!(editor.read(cx).value().as_str(), "**hello** world");
            assert!(editor.focus_handle(cx).is_focused(window));
        })
        .unwrap();
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-z"
            } else {
                "ctrl-z"
            },
        );
        draw(cx, handle);
        assert_eq!(
            cx.update(|cx| editor.read(cx).value().to_string()),
            "hello world"
        );
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.simulate_input(handle, "todo");
        cx.simulate_keystrokes(handle, "escape");
        draw(cx, handle);
        assert!(cx.update(|cx| view.read(cx).palette.is_none()));
        assert_eq!(
            cx.update(|cx| editor.read(cx).value().to_string()),
            "hello world"
        );
    }

    #[gpui_kit::test]
    fn image_picker_imports_at_the_saved_selection(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("enso-palette-image-{}", std::process::id()));
        std::fs::create_dir_all(root.join("notes")).unwrap();
        let source = root.join("picture.png");
        std::fs::write(&source, include_bytes!("../assets/logo-light.png")).unwrap();
        let note = root.join("notes/note.md");
        std::fs::write(&note, "before after").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.root = Some(root.clone());
                    app.add_document(Some(note.clone()), "before after".into(), None, window, cx);
                    app.mode = Mode::Edit;
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        let editor = cx.update(|cx| view.read(cx).documents[0].editor.clone());
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |s, cx| s.set_selected_range(7..7, cx));
            editor.focus_handle(cx).focus(window, cx);
        })
        .unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.simulate_input(handle, "image");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|_| Some(vec![source.clone()]));
        cx.run_until_parked();
        let saved = root.join(".assets/images/picture.png");
        assert_eq!(
            std::fs::read(saved).unwrap(),
            std::fs::read(source).unwrap()
        );
        cx.update(|cx| {
            assert!(view.read(cx).palette.is_none());
            assert_eq!(editor.read(cx).value().as_str(), "before <img src=\"../.assets/images/picture.png\" alt=\"picture\" width=\"100%\" />after");
        });
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui_kit::test]
    fn table_prompt_and_context_filter(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(super::History::default());
            bind_view_keys(cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(window, cx);
                    app.add_document(
                        Some(PathBuf::from("table.md")),
                        String::new(),
                        None,
                        window,
                        cx,
                    );
                    app
                })
            })
            .unwrap()
        });
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.update(|cx| {
            let p = view.read(cx).palette.as_ref().unwrap();
            assert!(p.groups[1].is_empty() && p.groups[2].is_empty());
        });
        cx.simulate_keystrokes(handle, "escape");
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |app, cx| app.set_mode(Mode::Edit, window, cx))
        })
        .unwrap();
        draw(cx, handle);
        cx.simulate_keystrokes(
            handle,
            if cfg!(target_os = "macos") {
                "cmd-shift-p"
            } else {
                "ctrl-shift-p"
            },
        );
        draw(cx, handle);
        cx.simulate_input(handle, "table");
        draw(cx, handle);
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        assert!(cx.update(|cx| {
            view.read(cx)
                .palette
                .as_ref()
                .unwrap()
                .table_input
                .is_some()
        }));
        cx.simulate_keystrokes(handle, "enter");
        draw(cx, handle);
        cx.update(|cx| {
            assert!(view.read(cx).palette.is_none());
            let text = view.read(cx).documents[0].editor.read(cx).value();
            assert!(text.contains("Column 3"));
        });
    }
}
