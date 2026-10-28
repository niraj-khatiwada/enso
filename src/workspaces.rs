//! Folders the user has opened as workspaces, most recent first.
//!
//! Stored as plain lines of `<unix seconds>\t<path>` beside the appearance
//! preference. A workspace that has since been moved or deleted stays listed
//! (it may live on a drive that is only unplugged) until the user removes it.

use gpui_kit::{App, BorrowAppContext, Global};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct Recent {
    pub path: PathBuf,
    pub opened: u64,
}

impl Recent {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }

    pub fn exists(&self) -> bool {
        self.path.is_dir()
    }
}

#[derive(Default)]
pub struct Workspaces {
    pub recent: Vec<Recent>,
    file: Option<PathBuf>,
}

impl Global for Workspaces {}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("Enso").join("workspaces"))
}

fn parse(text: &str) -> Vec<Recent> {
    let mut recent: Vec<Recent> = text
        .lines()
        .filter_map(|line| {
            let (opened, path) = line.split_once('\t')?;
            Some(Recent {
                opened: opened.parse().ok()?,
                path: PathBuf::from(path),
            })
        })
        .collect();
    recent.sort_by(|a, b| b.opened.cmp(&a.opened));
    let mut seen = std::collections::HashSet::new();
    recent.retain(|r| seen.insert(r.path.clone()));
    recent
}

pub fn init(cx: &mut App) {
    let file = file();
    let recent = file
        .as_deref()
        .and_then(|f| fs::read_to_string(f).ok())
        .map(|text| parse(&text))
        .unwrap_or_default();
    cx.set_global(Workspaces { recent, file });
}

impl Workspaces {
    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let text: String = self
            .recent
            .iter()
            .map(|r| format!("{}\t{}\n", r.opened, r.path.display()))
            .collect();
        let _ = fs::write(file, text);
    }
}

pub fn recent(cx: &App) -> Vec<Recent> {
    cx.try_global::<Workspaces>()
        .map(|w| w.recent.clone())
        .unwrap_or_default()
}

/// The most recently opened workspace, if it was recorded at all, and whether
/// it is still there.
pub fn last(cx: &App) -> Option<Recent> {
    cx.try_global::<Workspaces>()
        .and_then(|w| w.recent.first().cloned())
}

/// Move `path` to the top of the list.
pub fn record(path: &Path, cx: &mut App) {
    let opened = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    cx.default_global::<Workspaces>();
    cx.update_global::<Workspaces, _>(|w, _| {
        w.recent.retain(|r| r.path != path);
        w.recent.insert(
            0,
            Recent {
                path: path.to_path_buf(),
                opened,
            },
        );
        w.save();
    });
}

pub fn forget(path: &Path, cx: &mut App) {
    cx.default_global::<Workspaces>();
    cx.update_global::<Workspaces, _>(|w, _| {
        w.recent.retain(|r| r.path != path);
        w.save();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_newest_first_and_skips_junk() {
        let recent = parse("10\t/a\nnot a line\n30\t/b\n20\t/a with space\n");
        let paths: Vec<_> = recent.iter().map(|r| r.path.clone()).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("/b"),
                PathBuf::from("/a with space"),
                PathBuf::from("/a")
            ]
        );
    }
}
