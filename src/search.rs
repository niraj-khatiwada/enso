//! Search inside every note of a workspace ("Search everything" in the
//! palette).
//!
//! Notes workspaces are small enough to scan directly: file contents are kept
//! in memory between searches and re-read only when a file's modified time or
//! size changes, the scan is split across CPU cores, and a newer query stops
//! an older one through `cancel`.

use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

/// At most this many matching lines are returned, and this many per file.
pub const LIMIT: usize = 200;
const PER_FILE: usize = 20;
/// Larger files aren't notes; they're skipped.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Matched lines longer than this are cut around the first match.
const LINE_CHARS: usize = 160;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: PathBuf,
    /// 1-based.
    pub line_number: usize,
    /// Byte offset of the first match in the file, to put the cursor there.
    pub offset: usize,
    /// The line as shown, trimmed and cut to a readable length.
    pub line: String,
    /// Byte ranges of the matched words within `line`.
    pub ranges: Vec<Range<usize>>,
}

/// File texts from earlier searches, with the stamp they were read at.
#[derive(Default)]
pub struct Cache(Mutex<HashMap<PathBuf, (SystemTime, u64, Arc<String>)>>);

impl Cache {
    fn text(&self, path: &Path) -> Option<Arc<String>> {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > MAX_FILE_BYTES {
            return None;
        }
        let stamp = (meta.modified().ok()?, meta.len());
        if let Some((time, len, text)) = self.0.lock().ok()?.get(path) {
            if (*time, *len) == stamp {
                return Some(text.clone());
            }
        }
        let text = Arc::new(crate::files::read_text(path).ok()?);
        self.0
            .lock()
            .ok()?
            .insert(path.to_path_buf(), (stamp.0, stamp.1, text.clone()));
        Some(text)
    }
}

/// Files worth searching: text and Markdown, by extension.
pub fn is_searchable(path: &Path) -> bool {
    crate::files::is_markdown(path)
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "txt" | "text"))
}

/// The query's words. Matching ignores case unless the query has a capital.
fn terms(query: &str) -> (Vec<String>, bool) {
    let sensitive = query.chars().any(char::is_uppercase);
    let terms = query
        .split_whitespace()
        .map(|t| if sensitive { t.to_string() } else { t.to_lowercase() })
        .collect();
    (terms, sensitive)
}

/// Every line of `text` that holds all of `terms`, in any order.
fn search_text(path: &Path, text: &str, terms: &[String], sensitive: bool, hits: &mut Vec<Hit>) {
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += line.len();
        let line = line.trim_end_matches(['\n', '\r']);
        // Lowercasing can change byte lengths (rare letters); such lines are
        // matched on the original text so ranges stay valid.
        let folded = if sensitive { None } else { Some(line.to_lowercase()) };
        let haystack = match &folded {
            Some(f) if f.len() == line.len() => f.as_str(),
            _ => line,
        };
        let mut ranges = Vec::new();
        let mut found = true;
        for term in terms {
            let Some(at) = haystack.find(term.as_str()) else {
                found = false;
                break;
            };
            ranges.push(at..at + term.len());
        }
        if !found {
            continue;
        }
        ranges.sort_by_key(|r| r.start);
        let first = ranges[0].start;
        let (shown, shift) = excerpt(line, first);
        let ranges = ranges
            .into_iter()
            .filter_map(|r| {
                let r = r.start.checked_sub(shift)?..(r.end - shift).min(shown.len());
                (r.start < r.end).then_some(r)
            })
            .collect();
        hits.push(Hit {
            path: path.to_path_buf(),
            line_number: index + 1,
            offset: start + first,
            line: shown,
            ranges,
        });
        if hits.len() >= PER_FILE {
            return;
        }
    }
}

/// The part of `line` to show around byte `at`, and how many bytes were cut
/// from its front (leading whitespace included).
fn excerpt(line: &str, at: usize) -> (String, usize) {
    let indent = line.len() - line.trim_start().len();
    let mut start = indent.min(at);
    if line[start..].chars().count() > LINE_CHARS {
        // Keep some words before the match, then cut at the display length.
        let mut before = at;
        for (count, (i, _)) in line[..at].char_indices().rev().enumerate() {
            before = i;
            if count >= 30 {
                break;
            }
        }
        start = start.max(before);
    }
    let end = line[start..]
        .char_indices()
        .nth(LINE_CHARS)
        .map_or(line.len(), |(i, _)| start + i);
    (line[start..end].to_string(), start)
}

/// Search `files` for `query`. Results follow the files' order, at most
/// `LIMIT` lines. Returns early, with what it has, once `cancel` is set.
pub fn search(files: &[PathBuf], query: &str, cache: &Cache, cancel: &AtomicBool) -> Vec<Hit> {
    let (terms, sensitive) = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 8);
    let chunk = files.len().div_ceil(threads).max(1);
    let mut per_chunk: Vec<Vec<Hit>> = std::thread::scope(|scope| {
        files
            .chunks(chunk)
            .map(|files| {
                let terms = &terms;
                scope.spawn(move || {
                    let mut hits = Vec::new();
                    for path in files {
                        if cancel.load(Ordering::Relaxed) || hits.len() >= LIMIT {
                            break;
                        }
                        if let Some(text) = cache.text(path) {
                            search_text(path, &text, terms, sensitive, &mut hits);
                        }
                    }
                    hits
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    let mut hits: Vec<Hit> = per_chunk.iter_mut().flat_map(std::mem::take).collect();
    hits.truncate(LIMIT);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(text: &str, query: &str) -> Vec<Hit> {
        let (terms, sensitive) = terms(query);
        let mut hits = Vec::new();
        search_text(Path::new("n.md"), text, &terms, sensitive, &mut hits);
        hits
    }

    #[test]
    fn every_word_on_the_line_in_any_order() {
        let found = hits("# Groceries\n  buy milk and eggs\neggs only\n", "eggs milk");
        assert_eq!(found.len(), 1);
        let hit = &found[0];
        assert_eq!((hit.line_number, hit.line.as_str()), (2, "buy milk and eggs"));
        // The cursor lands on the first match in the file.
        assert_eq!(&"# Groceries\n  buy milk and eggs\n"[hit.offset..hit.offset + 4], "milk");
        let words: Vec<_> = hit.ranges.iter().map(|r| &hit.line[r.clone()]).collect();
        assert_eq!(words, ["milk", "eggs"]);
    }

    #[test]
    fn case_is_ignored_until_the_query_has_a_capital() {
        assert_eq!(hits("Rust and rust\n", "rust").len(), 1);
        assert_eq!(hits("rust only\n", "Rust").len(), 0);
        assert_eq!(hits("Rust\n", "Rust").len(), 1);
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{} needle {}", "word ".repeat(100), "tail ".repeat(100));
        let hit = &hits(&line, "needle")[0];
        assert!(hit.line.chars().count() <= LINE_CHARS);
        assert_eq!(&hit.line[hit.ranges[0].clone()], "needle");
    }

    #[test]
    fn searches_files_and_reuses_unchanged_text() {
        let root = std::env::temp_dir().join(format!("enso-search-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.md");
        let b = root.join("b.txt");
        std::fs::write(&a, "alpha\nshared word\n").unwrap();
        std::fs::write(&b, "shared too\n").unwrap();
        let cache = Cache::default();
        let cancel = AtomicBool::new(false);
        let files = vec![a.clone(), b.clone()];
        let found = search(&files, "shared", &cache, &cancel);
        assert_eq!(found.iter().map(|h| h.path.clone()).collect::<Vec<_>>(), files);
        std::fs::write(&a, "changed\n").unwrap();
        assert_eq!(search(&files, "changed", &cache, &cancel).len(), 1);
        cancel.store(true, Ordering::Relaxed);
        assert!(search(&files, "shared", &cache, &cancel).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
