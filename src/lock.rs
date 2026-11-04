//! Locked notes: pages that can be read but not changed, from the editor or
//! the reader. A lock belongs to the note (by its path) and is remembered,
//! one path per line, beside the other settings.

use gpui_kit::{App, BorrowAppContext, Global};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Locks {
    paths: HashSet<PathBuf>,
    file: Option<PathBuf>,
}

impl Global for Locks {}

pub fn init(cx: &mut App) {
    let file = dirs::config_dir().map(|p| crate::files::app_folder(p).join("locked-notes"));
    let paths = file
        .as_deref()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|text| text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default();
    cx.set_global(Locks { paths, file });
}

/// Whether the note at `path` is locked. Without `init` (tests), none are.
pub fn is_locked(path: &Path, cx: &App) -> bool {
    cx.try_global::<Locks>().is_some_and(|locks| locks.paths.contains(path))
}

pub fn set_locked(path: &Path, locked: bool, cx: &mut App) {
    if !cx.has_global::<Locks>() {
        return;
    }
    cx.update_global::<Locks, _>(|locks, _| {
        let changed = if locked {
            locks.paths.insert(path.to_path_buf())
        } else {
            locks.paths.remove(path)
        };
        if changed {
            locks.save();
        }
    });
}

/// A note (or a folder of them) was renamed or moved: its lock goes along.
pub fn moved(old: &Path, new: &Path, cx: &mut App) {
    if !cx.has_global::<Locks>() {
        return;
    }
    cx.update_global::<Locks, _>(|locks, _| {
        let moved: Vec<PathBuf> = locks.paths.iter().filter(|p| p.starts_with(old)).cloned().collect();
        if moved.is_empty() {
            return;
        }
        for path in moved {
            locks.paths.remove(&path);
            let rest = path.strip_prefix(old).unwrap_or(Path::new(""));
            locks.paths.insert(if rest.as_os_str().is_empty() { new.to_path_buf() } else { new.join(rest) });
        }
        locks.save();
    });
}

impl Locks {
    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut lines: Vec<String> = self.paths.iter().map(|p| p.display().to_string()).collect();
        lines.sort();
        let _ = std::fs::write(file, lines.join("\n") + "\n");
    }
}
