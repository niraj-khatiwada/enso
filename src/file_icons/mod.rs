//! Workspace-owned sidebar decoration. Markdown never contains UI metadata.
mod picker;
pub use picker::Picker;

use crate::links::Move;
use gpui_kit::assets::IconName;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path},
    sync::{Mutex, OnceLock},
};
use unicode_segmentation::UnicodeSegmentation;

const FILE: &str = ".assets/file-icons.json";
static WRITES: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Choice {
    Icon {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    Emoji {
        value: String,
    },
}

impl Choice {
    fn valid(&self) -> bool {
        match self {
            Self::Icon { name, color } => {
                icon_named(name).is_some() && color.as_ref().is_none_or(|c| valid_color(c))
            }
            Self::Emoji { value } => valid_emoji(value),
        }
    }
}

pub fn valid_emoji(value: &str) -> bool {
    value.len() <= 128
        && value.graphemes(true).count() == 1
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && value.chars().any(|c| !c.is_ascii())
}

fn valid_color(value: &str) -> bool {
    matches!(value.len(), 7 | 9)
        && value.starts_with('#')
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn color(value: &str) -> Option<Hsla> {
    if !valid_color(value) {
        return None;
    }
    let digits = value.strip_prefix('#')?;
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(if digits.len() == 8 {
        rgba(value).into()
    } else {
        rgb(value).into()
    })
}

pub fn catalog() -> &'static [(String, IconName)] {
    static ICONS: OnceLock<Vec<(String, IconName)>> = OnceLock::new();
    ICONS.get_or_init(|| {
        IconName::ALL
            .iter()
            .map(|icon| {
                let path = icon.path();
                (
                    path.trim_start_matches("icons/")
                        .trim_end_matches(".svg")
                        .to_string(),
                    *icon,
                )
            })
            .collect()
    })
}

pub fn icon_named(name: &str) -> Option<IconName> {
    catalog()
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, icon)| *icon)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Identity {
    device: u64,
    file: u64,
    created: Option<u64>,
}

fn identity(path: &Path) -> Option<Identity> {
    let metadata = fs::metadata(path).ok()?;
    let (device, file) = crate::links::file_identity(path, &metadata)?;
    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|t| t.as_nanos().try_into().ok());
    Some(Identity {
        device,
        file,
        created,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    icon: Choice,
    identity: Option<Identity>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Archived {
    path: String,
    entry: Entry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    version: u32,
    pub icons: BTreeMap<String, Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    archived: Vec<Archived>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            version: 1,
            icons: BTreeMap::new(),
            archived: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('\\')
        && key
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(':'))
}

fn key(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut names = Vec::new();
    for part in relative.components() {
        let Component::Normal(name) = part else {
            return None;
        };
        names.push(name.to_str()?);
    }
    let key = names.join("/");
    valid_key(&key).then_some(key)
}

fn read(root: &Path) -> io::Result<(Metadata, Option<String>)> {
    let text = match fs::read_to_string(root.join(FILE)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Metadata::default(), None)),
        Err(e) => return Err(e),
    };
    let data: Metadata =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if data.version != 1 {
        return Err(io::Error::other(
            "This workspace uses an unsupported file-icon metadata version.",
        ));
    }
    if data.icons.keys().any(|k| !valid_key(k)) || data.archived.iter().any(|a| !valid_key(&a.path))
    {
        return Err(io::Error::other(
            "File-icon metadata contains an invalid relative path.",
        ));
    }
    Ok((data, Some(text)))
}

pub fn load(root: &Path) -> io::Result<Metadata> {
    read(root).map(|(data, _)| data)
}

impl Metadata {
    pub fn get(&self, root: &Path, path: &Path) -> Option<&Choice> {
        self.icons
            .get(&key(root, path)?)
            .map(|entry| &entry.icon)
            .filter(|c| c.valid())
    }
}

fn mutate(
    root: &Path,
    change: impl FnOnce(&mut Metadata) -> io::Result<()>,
) -> io::Result<Metadata> {
    let _lock = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let (mut data, expected) = read(root)?;
    let before = data.clone();
    change(&mut data)?;
    if before != data {
        let text = serde_json::to_string_pretty(&data).map_err(io::Error::other)? + "\n";
        let path = root.join(FILE);
        fs::create_dir_all(path.parent().unwrap())?;
        if let Some(expected) = expected {
            crate::files::save(&path, &expected, &text)?;
        } else {
            crate::files::save_as(&path, &text)?;
        }
    }
    Ok(data)
}

pub fn set(root: &Path, path: &Path, choice: Option<Choice>) -> io::Result<Metadata> {
    let key =
        key(root, path).ok_or_else(|| io::Error::other("Choose a file inside this workspace."))?;
    if !path.is_file() {
        return Err(io::Error::other("This file is no longer available."));
    }
    if choice.as_ref().is_some_and(|c| !c.valid()) {
        return Err(io::Error::other("Choose a valid icon, color, or emoji."));
    }
    mutate(root, |data| {
        let id = identity(path);
        data.archived
            .retain(|a| a.entry.identity.is_none() || a.entry.identity != id);
        match choice {
            Some(icon) => {
                data.icons.insert(key, Entry { icon, identity: id });
            }
            None => {
                data.icons.remove(&key);
            }
        }
        Ok(())
    })
}

/// Apply known moves and copies before archiving missing files. Folder moves
/// map every descendant; restored files recover icons by identity, never name.
pub fn reconcile(root: &Path, moves: &[Move], copies: &[Move]) -> io::Result<Metadata> {
    reconcile_inner(root, moves, copies, None)
}

pub fn reconcile_with_index(
    root: &Path,
    moves: &[Move],
    copies: &[Move],
    index: &crate::links::Index,
) -> io::Result<Metadata> {
    reconcile_inner(root, moves, copies, Some(index))
}

fn reconcile_inner(
    root: &Path,
    moves: &[Move],
    copies: &[Move],
    index: Option<&crate::links::Index>,
) -> io::Result<Metadata> {
    mutate(root, |data| {
        let before = data.icons.clone();
        for (old, choice) in &before {
            let old_path = root.join(old);
            let new_path = crate::links::moved(&old_path, moves);
            if new_path != old_path {
                data.icons.remove(old);
                if let Some(new) = key(root, &new_path) {
                    data.icons.insert(new, choice.clone());
                }
            }
            for movement in copies {
                if let Ok(rest) = old_path.strip_prefix(&movement.old) {
                    let new = movement.new.join(rest);
                    if let Some(key) = key(root, &new) {
                        data.icons.entry(key).or_insert_with(|| Entry {
                            icon: choice.icon.clone(),
                            identity: identity(&new),
                        });
                    }
                }
            }
        }
        let mut missing = Vec::new();
        for (key, entry) in &mut data.icons {
            // Do not discard metadata when a synced volume is inaccessible.
            let path = root.join(key);
            if !path.try_exists()? {
                missing.push(key.clone());
            } else {
                entry.identity = identity(&path);
            }
        }
        for path in missing {
            if let Some(entry) = data.icons.remove(&path) {
                data.archived.push(Archived { path, entry });
            }
        }
        let mut kept = Vec::new();
        for archived in data.archived.drain(..) {
            let candidate = archived
                .entry
                .identity
                .as_ref()
                .and_then(|id| index.and_then(|i| i.path_for_identity((id.device, id.file))))
                .cloned()
                .unwrap_or_else(|| root.join(&archived.path));
            let restored = archived
                .entry
                .identity
                .as_ref()
                .is_some_and(|id| Some(id) == identity(&candidate).as_ref());
            if restored && let Some(key) = key(root, &candidate) {
                data.icons.entry(key).or_insert(archived.entry);
            } else {
                kept.push(archived);
            }
        }
        data.archived = kept;
        Ok(())
    })
}

use crate::Enso;
use gpui_kit::{
    component::{
        ActiveTheme, Icon, Sizable,
        button::{Button, ButtonVariants},
        popover::Popover,
    },
    *,
};

pub fn render(choice: Option<&Choice>, default: IconName, color: Hsla) -> AnyElement {
    match choice {
        Some(Choice::Emoji { value }) => div()
            .size(px(20.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(14.))
            .line_height(relative(1.))
            .child(value.clone())
            .into_any_element(),
        Some(Choice::Icon { name, color: tint }) => Icon::new(icon_named(name).unwrap_or(default))
            .size(px(16.))
            .text_color(tint.as_ref().and_then(|c| self::color(c)).unwrap_or(color))
            .into_any_element(),
        None => Icon::new(default)
            .size(px(16.))
            .text_color(color)
            .into_any_element(),
    }
}

impl Enso {
    pub(crate) fn apply_file_icon(
        &mut self,
        root: &Path,
        path: &Path,
        choice: Option<Choice>,
        close: bool,
        cx: &mut Context<Self>,
    ) {
        if self.root.as_deref() != Some(root) {
            return;
        }
        if self.icon_picker.as_ref().is_none_or(|(p, _)| p != path) {
            return;
        }
        match set(root, path, choice) {
            Ok(data) => {
                self.file_icons = data;
                if close {
                    self.icon_picker = None;
                }
                cx.notify();
            }
            Err(error) => self.message(format!("Could not save the file icon: {error}"), true, cx),
        }
    }

    pub(crate) fn refresh_file_icons(
        &mut self,
        result: io::Result<Metadata>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(data) => {
                self.icon_error = None;
                if self.file_icons != data {
                    self.file_icons = data;
                    cx.notify();
                }
            }
            Err(error) => {
                let error = format!("Could not load file icons: {error}");
                if self.icon_error.as_ref() != Some(&error) {
                    self.message(error.clone(), true, cx);
                    self.icon_error = Some(error);
                }
            }
        }
    }

    pub(crate) fn file_icon_button(&self, path: &Path, cx: &mut Context<Self>) -> AnyElement {
        let Some(root) = self.root.clone() else {
            return render(None, IconName::File, cx.theme().muted_foreground);
        };
        let path = path.to_path_buf();
        let choice = self.file_icons.get(&root, &path);
        let picker = self
            .icon_picker
            .as_ref()
            .filter(|(p, _)| p == &path)
            .map(|(_, picker)| picker.clone());
        let open = picker.is_some();
        let color_open = picker
            .as_ref()
            .is_some_and(|picker| picker.read(cx).color_picker.read(cx).is_open());
        let owner = cx.entity().downgrade();
        let trigger = Button::new(SharedString::from(format!("file-icon:{}", path.display())))
            .ghost()
            .xsmall()
            .compact()
            .w(px(24.))
            .h(px(24.))
            .p_0()
            .on_click(|_, _, cx| cx.stop_propagation())
            .tooltip("Change Icon")
            .accessibility_label("Change Icon")
            .child(render(choice, IconName::File, cx.theme().muted_foreground));
        let popover = Popover::new(SharedString::from(format!(
            "file-icon-popover:{}",
            path.display()
        )))
        .open(open)
        .overlay_closable(!color_open)
        .p_2()
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            let _ = owner.update(cx, |app, cx| {
                if *open {
                    let current = app.file_icons.get(&root, &path).cloned();
                    let owner = cx.entity().downgrade();
                    let picker = cx.new(|cx| {
                        Picker::new(root.clone(), path.clone(), current, owner, window, cx)
                    });
                    app.icon_picker = Some((path.clone(), picker));
                } else if app.icon_picker.as_ref().is_some_and(|(p, _)| p == &path) {
                    app.icon_picker = None;
                }
                cx.notify();
            });
        })
        .content(move |_, _, _| div().children(picker.clone()));
        div()
            .debug_selector(|| "sidebar-file-icon".into())
            .child(popover)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    use std::path::PathBuf;

    struct Workspace(PathBuf);
    impl Workspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "enso-icons-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn note(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "# Keep the Markdown unchanged\n").unwrap();
            path
        }
    }
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn star() -> Choice {
        Choice::Icon {
            name: "star".into(),
            color: Some("#498add".into()),
        }
    }

    #[test]
    fn choices_persist_relative_to_the_workspace_without_touching_markdown() {
        let ws = Workspace::new();
        let note = ws.note("Folder/日本語.md");
        let before = fs::read(&note).unwrap();
        set(&ws.0, &note, Some(star())).unwrap();
        let data = load(&ws.0).unwrap();
        assert_eq!(data.get(&ws.0, &note), Some(&star()));
        assert!(data.icons.contains_key("Folder/日本語.md"));
        assert_eq!(fs::read(&note).unwrap(), before);
        let moved_root = ws.0.with_extension("moved");
        fs::rename(&ws.0, &moved_root).unwrap();
        assert_eq!(
            load(&moved_root)
                .unwrap()
                .get(&moved_root, &moved_root.join("Folder/日本語.md")),
            Some(&star())
        );
        fs::rename(moved_root, &ws.0).unwrap();
        set(&ws.0, &note, None).unwrap();
        assert!(load(&ws.0).unwrap().get(&ws.0, &note).is_none());
    }

    #[test]
    fn folder_moves_and_copies_keep_independent_icons() {
        let ws = Workspace::new();
        let old = ws.note("Folder/note.md");
        set(&ws.0, &old, Some(star())).unwrap();
        let new = ws.0.join("Renamed/note.md");
        fs::rename(ws.0.join("Folder"), ws.0.join("Renamed")).unwrap();
        let data = reconcile(
            &ws.0,
            &[Move {
                old: ws.0.join("Folder"),
                new: ws.0.join("Renamed"),
            }],
            &[],
        )
        .unwrap();
        assert_eq!(data.get(&ws.0, &new), Some(&star()));
        assert!(data.get(&ws.0, &old).is_none());
        let copy = ws.note("copy.md");
        let data = reconcile(
            &ws.0,
            &[],
            &[Move {
                old: new.clone(),
                new: copy.clone(),
            }],
        )
        .unwrap();
        assert_eq!(data.get(&ws.0, &copy), Some(&star()));
        set(&ws.0, &new, None).unwrap();
        assert_eq!(load(&ws.0).unwrap().get(&ws.0, &copy), Some(&star()));
    }

    #[test]
    fn trash_restore_recovers_the_icon_but_a_replacement_file_does_not() {
        let ws = Workspace::new();
        let note = ws.note("note.md");
        set(&ws.0, &note, Some(star())).unwrap();
        let trash = ws.0.join(".simulated-trash");
        fs::create_dir(&trash).unwrap();
        fs::rename(&note, trash.join("note.md")).unwrap();
        let data = reconcile(&ws.0, &[], &[]).unwrap();
        assert!(data.get(&ws.0, &note).is_none());
        assert_eq!(data.archived.len(), 1);
        ws.note("note.md");
        let data = reconcile(&ws.0, &[], &[]).unwrap();
        assert!(
            data.get(&ws.0, &note).is_none(),
            "a new file must not inherit the archived icon"
        );
        let restored = ws.0.join("restored.md");
        fs::rename(trash.join("note.md"), &restored).unwrap();
        let mut index = crate::links::Index::default();
        index.observe(&ws.0, Vec::new());
        let data = reconcile_with_index(&ws.0, &[], &[], &index).unwrap();
        assert_eq!(data.get(&ws.0, &restored), Some(&star()));
        assert!(data.get(&ws.0, &note).is_none());
        assert!(data.archived.is_empty());
    }

    #[test]
    fn atomic_saves_and_external_renames_keep_icons() {
        let ws = Workspace::new();
        let note = ws.note("note.md");
        set(&ws.0, &note, Some(star())).unwrap();
        crate::files::save(&note, "# Keep the Markdown unchanged\n", "Changed.\n").unwrap();
        let data = reconcile(&ws.0, &[], &[]).unwrap();
        assert_eq!(data.get(&ws.0, &note), Some(&star()));
        let renamed = ws.0.join("renamed.md");
        fs::rename(note, &renamed).unwrap();
        let mut index = crate::links::Index::default();
        index.observe(&ws.0, Vec::new());
        let data = reconcile_with_index(&ws.0, &[], &[], &index).unwrap();
        assert_eq!(data.get(&ws.0, &renamed), Some(&star()));
    }

    #[test]
    fn malformed_or_future_metadata_is_not_overwritten() {
        let ws = Workspace::new();
        let note = ws.note("note.md");
        fs::create_dir(ws.0.join(".assets")).unwrap();
        for text in [
            "{ broken json",
            "{\"version\":2,\"icons\":{}}",
            "{\"version\":1,\"icons\":{\"../outside.md\":{\"icon\":{\"kind\":\"emoji\",\"value\":\"🔥\"},\"identity\":null}}}",
        ] {
            fs::write(ws.0.join(FILE), text).unwrap();
            assert!(set(&ws.0, &note, Some(star())).is_err());
            assert_eq!(fs::read_to_string(ws.0.join(FILE)).unwrap(), text);
        }
    }

    #[test]
    fn emoji_sequences_and_icon_colors_are_validated() {
        for value in ["🔥", "🇳🇵", "👩🏽‍💻", "🏳️‍🌈", "1️⃣"] {
            assert!(valid_emoji(value), "{value}");
        }
        for value in ["", "two words", "a", "🔥🔥", "\n"] {
            assert!(!valid_emoji(value), "{value}");
        }
        assert!(star().valid());
        assert!(
            !Choice::Icon {
                name: "not-a-bundled-icon".into(),
                color: None
            }
            .valid()
        );
        assert!(
            !Choice::Icon {
                name: "star".into(),
                color: Some("#xyzxyz".into())
            }
            .valid()
        );
    }
}
