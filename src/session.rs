//! What each workspace had open when it was last used: its tabs and pins, the active
//! one, the view mode and the sidebar, so reopening the workspace picks up
//! where the user left off.
//!
//! Stored beside the workspace history as plain lines, one block per
//! workspace:
//!
//! ```text
//! workspace	/path/to/folder
//! mode	split
//! sidebar	open	248
//! active	1
//! tab	/path/to/folder/a.md
//! tab	/path/to/folder/b.md
//! open	/path/to/folder/notes
//! ```
//!
//! Only saved files are kept: an unsaved note has no file to reopen.

use gpui_kit::{App, BorrowAppContext, Global};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// How many workspaces keep a session; the least recently saved go first.
const KEEP: usize = 50;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub tabs: Vec<PathBuf>,
    /// Paths, rather than indices, so missing files cannot shift another pin.
    pub pinned: Vec<PathBuf>,
    pub active: Option<usize>,
    /// "read", "edit" or "split".
    pub mode: String,
    pub sidebar_open: bool,
    pub sidebar_width: f32,
    /// Folders expanded in the sidebar.
    pub expanded: Vec<PathBuf>,
}

#[derive(Default)]
pub struct Sessions {
    /// Most recently saved first.
    entries: Vec<(PathBuf, Session)>,
    file: Option<PathBuf>,
}

impl Global for Sessions {}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("sessions"))
}

fn parse(text: &str) -> Vec<(PathBuf, Session)> {
    let mut entries: Vec<(PathBuf, Session)> = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let key = parts.next().unwrap_or_default();
        if key == "workspace" {
            if let Some(path) = parts.next() {
                entries.push((
                    PathBuf::from(path),
                    Session { sidebar_open: true, sidebar_width: 248., ..Default::default() },
                ));
            }
            continue;
        }
        let Some((_, session)) = entries.last_mut() else {
            continue;
        };
        match key {
            "mode" => session.mode = parts.next().unwrap_or_default().to_string(),
            "sidebar" => {
                session.sidebar_open = parts.next() != Some("closed");
                if let Some(width) = parts.next().and_then(|w| w.parse().ok()) {
                    session.sidebar_width = width;
                }
            }
            "active" => session.active = parts.next().and_then(|i| i.parse().ok()),
            "tab" => {
                if let Some(path) = parts.next() {
                    session.tabs.push(PathBuf::from(path));
                }
            }
            "pin" => {
                if let Some(path) = parts.next() {
                    session.pinned.push(PathBuf::from(path));
                }
            }
            "open" => {
                if let Some(path) = parts.next() {
                    session.expanded.push(PathBuf::from(path));
                }
            }
            _ => {}
        }
    }
    let mut seen = std::collections::HashSet::new();
    entries.retain(|(path, _)| seen.insert(path.clone()));
    entries
}

fn format(entries: &[(PathBuf, Session)]) -> String {
    let mut text = String::new();
    for (workspace, session) in entries {
        text += &format!("workspace\t{}\n", workspace.display());
        text += &format!("mode\t{}\n", session.mode);
        text += &format!(
            "sidebar\t{}\t{}\n",
            if session.sidebar_open { "open" } else { "closed" },
            session.sidebar_width
        );
        if let Some(active) = session.active {
            text += &format!("active\t{active}\n");
        }
        for tab in &session.tabs {
            text += &format!("tab\t{}\n", tab.display());
        }
        for tab in &session.pinned {
            text += &format!("pin\t{}\n", tab.display());
        }
        for folder in &session.expanded {
            text += &format!("open\t{}\n", folder.display());
        }
    }
    text
}

pub fn init(cx: &mut App) {
    let file = file();
    let entries = file
        .as_deref()
        .and_then(|f| fs::read_to_string(f).ok())
        .map(|text| parse(&text))
        .unwrap_or_default();
    cx.set_global(Sessions { entries, file });
}

/// The session `workspace` was left with, if one was saved. Without `init`
/// (as in tests) nothing is remembered.
pub fn get(workspace: &Path, cx: &App) -> Option<Session> {
    cx.try_global::<Sessions>()?
        .entries
        .iter()
        .find(|(path, _)| path == workspace)
        .map(|(_, session)| session.clone())
}

/// Remember `session` for `workspace`, if it changed.
pub fn save(workspace: &Path, session: Session, cx: &mut App) {
    if !cx.has_global::<Sessions>() || get(workspace, cx).as_ref() == Some(&session) {
        return;
    }
    cx.update_global::<Sessions, _>(|sessions, _| {
        sessions.entries.retain(|(path, _)| path != workspace);
        sessions.entries.insert(0, (workspace.to_path_buf(), session));
        sessions.entries.truncate(KEEP);
        let Some(file) = &sessions.file else {
            return;
        };
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(file, format(&sessions.entries));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_round_trip_and_skip_junk() {
        let entries = vec![
            (
                PathBuf::from("/notes"),
                Session {
                    tabs: vec![PathBuf::from("/notes/a.md"), PathBuf::from("/notes/b c.md")],
                    pinned: vec![PathBuf::from("/notes/a.md")],
                    active: Some(1),
                    mode: "split".into(),
                    sidebar_open: false,
                    sidebar_width: 300.,
                    expanded: vec![PathBuf::from("/notes/deep"), PathBuf::from("/notes/deep/er")],
                },
            ),
            (
                PathBuf::from("/work"),
                Session { mode: "read".into(), sidebar_open: true, sidebar_width: 248., ..Default::default() },
            ),
        ];
        assert_eq!(parse(&format(&entries)), entries);
        // Lines before any workspace, and unknown keys, are ignored.
        let parsed = parse("tab\t/stray.md\nworkspace\t/w\nwhat\tever\ntab\t/w/x.md\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].1.tabs, [PathBuf::from("/w/x.md")]);
        assert!(parsed[0].1.pinned.is_empty(), "older sessions have no pinned tabs");
    }
}
