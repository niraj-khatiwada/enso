//! Workspace link graph. Only link spans and file stamps survive a scan; large
//! notes are streamed through a read-only mapping, never retained as strings.
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Wiki,
    Markdown,
    Html,
}
#[derive(Clone, Debug)]
pub struct Link {
    pub full: Range<usize>,
    pub range: Range<usize>, // destination only, preserving aliases, titles and HTML
    pub value: String,
    pub kind: Kind,
}
#[derive(Clone, Debug)]
pub struct Patch {
    pub range: Range<usize>,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Debug)]
pub struct Note {
    pub path: PathBuf,
    pub name: String,
    pub label: String,
}
#[derive(Clone, Default)]
pub struct Catalog {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    names: HashMap<String, Vec<PathBuf>>,
    paths: HashSet<PathBuf>,
}
impl Catalog {
    pub fn new(root: &Path, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut this = Self {
            root: root.into(),
            ..Default::default()
        };
        for path in paths {
            if !crate::files::is_markdown(&path) {
                continue;
            }
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let label = slash(path.strip_prefix(root).unwrap_or(&path));
            this.names
                .entry(name.to_lowercase())
                .or_default()
                .push(path.clone());
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            this.names
                .entry(file_name.to_lowercase())
                .or_default()
                .push(path.clone());
            this.paths.insert(path.clone());
            this.notes.push(Note { path, name, label });
        }
        this.notes.sort_by(|a, b| a.label.cmp(&b.label));
        this
    }
    /// Explicit relative path, then workspace path, then a unique basename.
    /// Duplicate basenames never select an arbitrary note in another folder.
    pub fn resolve(&self, from: &Path, value: &str) -> Option<PathBuf> {
        let value = value.split('|').next()?.split('#').next()?.trim();
        if value.is_empty() {
            return Some(from.to_owned());
        }
        let value = decode(value);
        let local = normalize(&from.parent()?.join(&value));
        let rooted = normalize(&self.root.join(value.trim_start_matches('/')));
        for path in [local, rooted] {
            if !path.starts_with(&self.root) {
                continue;
            }
            if self.paths.contains(&path) {
                return Some(path);
            }
            for extension in ["md", "markdown", "mdown", "mkd", "mdx"] {
                let with = PathBuf::from(format!("{}.{}", path.display(), extension));
                if self.paths.contains(&with) {
                    return Some(with);
                }
            }
        }
        let matches = self.names.get(&value.to_lowercase())?;
        (matches.len() == 1).then(|| matches[0].clone())
    }
    pub fn suggestions(&self, from: &Path, query: &str) -> Vec<Note> {
        let q = query.to_lowercase();
        let mut notes: Vec<_> = self
            .notes
            .iter()
            .filter(|n| {
                n.path != from
                    && (n.name.to_lowercase().contains(&q) || n.label.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        notes.sort_by_key(|n| {
            (
                !n.name.to_lowercase().starts_with(&q),
                n.name.to_lowercase(),
                n.label.clone(),
            )
        });
        notes.truncate(30);
        notes
    }
    pub fn destination(&self, from: &Path, to: &Path) -> String {
        let name = to.file_name().unwrap_or_default().to_string_lossy();
        if self.resolve(from, &name).as_deref() == Some(to) {
            return name.into_owned();
        }
        let rooted = slash(to.strip_prefix(&self.root).unwrap_or(to));
        if self.resolve(from, &rooted).as_deref() == Some(to) {
            rooted
        } else {
            // A nested folder may shadow the same workspace-relative path.
            crate::files::relative_link(from.parent().unwrap_or(&self.root), to)
        }
    }
}

pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(part),
        }
    }
    out
}
pub fn decode(value: &str) -> String {
    let value = value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("\\ ", " ")
        .replace("\\(", "(")
        .replace("\\)", ")");
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or(value)
}
pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn local_target(root: &Path, from: &Path, value: &str) -> Option<PathBuf> {
    let raw = value.split(['#', '?']).next()?.trim();
    if raw.is_empty() || raw.contains(':') || raw.starts_with("//") {
        return None;
    }
    let path = if raw.starts_with('/') {
        root.join(raw.trim_start_matches('/'))
    } else {
        from.parent()?.join(decode(raw))
    };
    Some(normalize(&path))
}
pub fn local_path(from: &Path, value: &str) -> Option<PathBuf> {
    let raw = value.split(['#', '?']).next()?;
    if raw.is_empty() || raw.contains(':') || raw.starts_with("//") {
        return None;
    }
    Some(normalize(&from.join(decode(raw))))
}

/// Linear lexical pass. Ignore fenced/indented code, inline code, escapes and
/// comments. Balanced destinations preserve nested parentheses and link titles.
pub fn scan(source: &str) -> Vec<Link> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut wiki_close = source.find("]]");
    while i < b.len() {
        if i == 0 || b[i - 1] == b'\n' {
            let end = source[i..].find('\n').map_or(b.len(), |e| i + e);
            let line = &source[i..end];
            let trim = line.trim_start_matches(' ');
            let indent = line.len() - trim.len();
            let marker = trim.as_bytes().first().copied().unwrap_or(0);
            let n = trim.bytes().take_while(|x| *x == marker).count();
            if indent <= 3 && matches!(marker, b'`' | b'~') && n >= 3 {
                match fence {
                    Some((c, len)) if c == marker && n >= len && trim[n..].trim().is_empty() => {
                        fence = None
                    }
                    None => fence = Some((marker, n)),
                    _ => {}
                }
                i = (end + 1).min(b.len());
                continue;
            }
            if fence.is_some() || indent >= 4 || line.starts_with('\t') {
                i = (end + 1).min(b.len());
                continue;
            }
        }
        if b[i] == b'\\' {
            i += 1;
            if i < b.len() {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
            continue;
        }
        if source[i..].starts_with("<!--") {
            i = source[i + 4..]
                .find("-->")
                .map_or(b.len(), |e| i + 4 + e + 3);
            continue;
        }
        if b[i] == b'`' {
            let n = b[i..].iter().take_while(|v| **v == b'`').count();
            let marker = &source[i..i + n];
            i = source[i + n..]
                .find(marker)
                .map_or(i + n, |e| i + n + e + n);
            continue;
        }
        if source[i..].starts_with("[[") {
            if wiki_close.is_some_and(|end| end < i + 2) {
                wiki_close = source[i + 2..].find("]]").map(|end| i + 2 + end);
            }
            if let Some(end) = wiki_close {
                if !source[i + 2..end].contains('\n') {
                    let target_end = source[i + 2..end].find('|').map_or(end, |v| i + 2 + v);
                    let value = source[i + 2..target_end].trim();
                    if !value.is_empty() {
                        let start = i + 2 + source[i + 2..target_end].find(value).unwrap();
                        out.push(Link {
                            full: i..end + 2,
                            range: start..start + value.len(),
                            value: value.into(),
                            kind: Kind::Wiki,
                        });
                    }
                    i = end + 2;
                    continue;
                }
            }
        }
        if b[i] == b'<' {
            // Raw code blocks and comments don't contribute links.
            let lower = source[i..]
                .get(..5.min(source.len() - i))
                .unwrap_or("")
                .to_ascii_lowercase();
            if lower.starts_with("<pre") || lower.starts_with("<code") || lower.starts_with("<scri")
            {
                let tag = if lower.starts_with("<pre") {
                    "pre"
                } else if lower.starts_with("<code") {
                    "code"
                } else {
                    "script"
                };
                if let Some(end) = source[i..].find(&format!("</{tag}>")) {
                    i += end + tag.len() + 3;
                    continue;
                }
            }
            if let Some(end) = source[i..].find('>') {
                let end = i + end + 1;
                scan_attributes(source, i..end, &mut out);
                i = end;
                continue;
            }
        }
        if b[i] == b']' && b.get(i + 1) == Some(&b'(') {
            if let Some(range) = destination(source, i + 2, false) {
                out.push(Link {
                    value: source[range.clone()].into(),
                    full: range.clone(),
                    range: range.clone(),
                    kind: Kind::Markdown,
                });
                i = range.end;
                continue;
            }
        }
        // Reference definitions: [image]: path "optional title".
        if b[i] == b']' && b.get(i + 1) == Some(&b':') {
            let start = source[..i].rfind('\n').map_or(0, |n| n + 1);
            if source[start..i].trim_start().starts_with('[')
                && !source[start..i].trim_start().starts_with("[^")
                && let Some(range) = destination(source, i + 2, true)
            {
                out.push(Link {
                    value: source[range.clone()].into(),
                    full: range.clone(),
                    range: range.clone(),
                    kind: Kind::Markdown,
                });
                i = range.end;
                continue;
            }
        }
        // ASCII delimiters above; advance Unicode safely.
        i += source[i..].chars().next().unwrap().len_utf8();
    }
    out
}
fn destination(s: &str, mut i: usize, definition: bool) -> Option<Range<usize>> {
    let b = s.as_bytes();
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        if b[i] == b'\n' {
            return None;
        }
        i += 1;
    }
    if b.get(i) == Some(&b'<') {
        let start = i + 1;
        let end = s[start..].find('>')? + start;
        return Some(start..end);
    }
    let start = i;
    let mut depth = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                i = (i + 2).min(b.len());
                continue;
            }
            b'(' => depth += 1,
            b')' if depth > 0 => depth -= 1,
            b')' if !definition => break,
            x if x.is_ascii_whitespace() => break,
            _ => {}
        }
        i += 1;
    }
    (i > start).then_some(start..i)
}
fn scan_attributes(s: &str, range: Range<usize>, out: &mut Vec<Link>) {
    let b = s.as_bytes();
    let mut i = range.start + 1;
    while i < range.end && !b[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < range.end {
        while i < range.end && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < range.end && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'_')) {
            i += 1;
        }
        if i == start {
            break;
        }
        let name = &s[start..i];
        while i < range.end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < range.end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = b.get(i).copied().filter(|c| matches!(c, b'\'' | b'"'));
        if quote.is_some() {
            i += 1;
        }
        let start = i;
        while i < range.end
            && if let Some(q) = quote {
                b[i] != q
            } else {
                !b[i].is_ascii_whitespace() && b[i] != b'>'
            }
        {
            i += 1;
        }
        if name.eq_ignore_ascii_case("src")
            || name.eq_ignore_ascii_case("href")
            || name.eq_ignore_ascii_case("poster")
        {
            out.push(Link {
                full: start..i,
                range: start..i,
                value: s[start..i].into(),
                kind: Kind::Html,
            });
        }
        if quote.is_some() {
            i += 1;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Move {
    pub old: PathBuf,
    pub new: PathBuf,
}
pub fn moved(path: &Path, moves: &[Move]) -> PathBuf {
    let mut result = path.to_owned();
    for m in moves {
        if let Ok(rest) = result.strip_prefix(&m.old) {
            result = if rest.as_os_str().is_empty() {
                m.new.clone()
            } else {
                m.new.join(rest)
            };
        }
    }
    result
}
pub fn original(path: &Path, moves: &[Move]) -> PathBuf {
    let mut result = path.to_owned();
    for m in moves.iter().rev() {
        if let Ok(rest) = result.strip_prefix(&m.new) {
            result = if rest.as_os_str().is_empty() {
                m.old.clone()
            } else {
                m.old.join(rest)
            };
        }
    }
    result
}
#[derive(Clone)]
pub struct Rules {
    pub before: Arc<Catalog>,
    pub after: Arc<Catalog>,
    pub moves: Vec<Move>,
}
impl Rules {
    pub fn patches(&self, from: &Path, links: &[Link]) -> Vec<Patch> {
        let new_from = moved(from, &self.moves);
        let mut patches = Vec::new();
        for link in links {
            let target = if link.kind == Kind::Wiki {
                self.before.resolve(from, &link.value)
            } else {
                local_target(&self.before.root, from, &link.value)
            };
            let Some(target) = target else { continue };
            let new_target = moved(&target, &self.moves);
            let value = if link.kind == Kind::Wiki {
                // Keep short names unchanged when moving didn't change resolution.
                if self.after.resolve(&new_from, &link.value).as_ref() == Some(&new_target) {
                    continue;
                }
                let suffix = link.value.find('#').map(|i| &link.value[i..]).unwrap_or("");
                let mut dest = self.after.destination(&new_from, &new_target);
                if !crate::files::is_markdown(Path::new(link.value.split('#').next().unwrap_or("")))
                {
                    // Existing extensionless wiki links retain their style.
                    dest = slash(&Path::new(&dest).with_extension(""));
                }
                format!("{dest}{suffix}")
            } else {
                if new_from.parent() == from.parent() && new_target == target {
                    continue;
                }
                let suffix = link
                    .value
                    .find(['#', '?'])
                    .map(|i| &link.value[i..])
                    .unwrap_or("");
                let raw = if link.value.starts_with('/') {
                    format!(
                        "/{}",
                        slash(
                            new_target
                                .strip_prefix(&self.after.root)
                                .unwrap_or(&new_target)
                        )
                    )
                } else {
                    crate::files::relative_link(
                        new_from.parent().unwrap_or(&self.after.root),
                        &new_target,
                    )
                };
                let encoded = encode(&raw);
                if link.kind == Kind::Html {
                    format!("{}{}", encoded.replace('&', "&amp;"), suffix)
                } else {
                    format!("{encoded}{suffix}")
                }
            };
            if value != link.value {
                patches.push(Patch {
                    range: link.range.clone(),
                    before: link.value.clone(),
                    after: value,
                });
            }
        }
        patches
    }
}
pub fn apply(source: &str, patches: &[Patch]) -> Option<String> {
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for p in patches {
        if p.range.start < at || source.get(p.range.clone())? != p.before {
            return None;
        }
        out.push_str(&source[at..p.range.start]);
        out.push_str(&p.after);
        at = p.range.end;
    }
    out.push_str(&source[at..]);
    Some(out)
}

#[derive(Clone)]
struct Record {
    stamp: (SystemTime, u64),
    links: Vec<Link>,
    open_revision: Option<u64>,
    fingerprint: u128,
}
#[derive(Default)]
pub struct Index {
    records: HashMap<PathBuf, Record>,
    pub catalog: Arc<Catalog>,
    pub backlinks: HashMap<PathBuf, Vec<PathBuf>>,
    incoming: HashMap<PathBuf, BTreeSet<PathBuf>>,
    pub parsed_files: usize,
    files: HashMap<PathBuf, FileInfo>,
    staged: HashMap<PathBuf, Record>,
}
/// No symlink traversal; hidden implementation folders stay out, except assets
/// can still be addressed by an ordinary relative URL.
fn inventory(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() || e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if kind.is_dir() {
                dirs.push(e.path())
            } else if kind.is_file() && crate::files::is_markdown(&e.path()) {
                files.push(e.path())
            }
        }
    }
    files.sort();
    files
}
fn with_text<T>(path: &Path, f: impl FnOnce(&str) -> T) -> std::io::Result<T> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() == 0 {
        return Ok(f(""));
    }
    // Read-only mappings are short-lived on the worker. Never retain mapped
    // pages in the graph, nor map a file while enso is replacing it.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let text = std::str::from_utf8(&map).map_err(std::io::Error::other)?;
    Ok(f(text))
}
impl Index {
    pub fn refresh(&mut self, root: &Path) -> bool {
        self.refresh_as(root, &[])
    }
    fn refresh_as(&mut self, root: &Path, moves: &[Move]) -> bool {
        let paths = inventory(root);
        let mut changed = false;
        let mut keep = HashSet::new();
        for actual in &paths {
            let path = original(actual, moves);
            keep.insert(path.clone());
            let Ok(meta) = std::fs::metadata(actual) else {
                continue;
            };
            let Ok(time) = meta.modified() else { continue };
            let stamp = (time, meta.len());
            if self.records.get(&path).is_some_and(|r| r.stamp == stamp) {
                continue;
            }
            let record = self
                .staged
                .remove(actual)
                .map(Ok)
                .unwrap_or_else(|| with_text(actual, |text| make_record(stamp, text)));
            if let Ok(record) = record {
                self.records.insert(path, record);
                self.parsed_files += 1;
                changed = true;
            }
        }
        let len = self.records.len();
        self.records.retain(|p, _| keep.contains(p));
        changed |= self.records.len() != len;
        if changed || self.catalog.root != root || !moves.is_empty() {
            self.catalog = Arc::new(Catalog::new(root, paths.iter().map(|p| original(p, moves))));
            self.rebuild();
            changed = true;
        }
        changed
    }
    pub fn rebuild_open(&mut self) {
        self.rebuild();
    }
    fn rebuild(&mut self) {
        self.incoming.clear();
        self.backlinks.clear();
        for (from, record) in &self.records {
            for link in &record.links {
                let target = if link.kind == Kind::Wiki {
                    self.catalog.resolve(from, &link.value)
                } else {
                    local_target(&self.catalog.root, from, &link.value)
                };
                if let Some(target) = target {
                    self.incoming
                        .entry(target)
                        .or_default()
                        .insert(from.clone());
                }
            }
        }
        for (target, sources) in &self.incoming {
            if crate::files::is_markdown(target) {
                self.backlinks.insert(
                    target.clone(),
                    sources.iter().filter(|p| *p != target).cloned().collect(),
                );
            }
        }
    }
    pub fn has_open_revision(&self, path: &Path, revision: u64) -> bool {
        self.records
            .get(path)
            .is_some_and(|r| r.open_revision == Some(revision))
    }
    pub fn update_open_revision(&mut self, path: &Path, text: &str, revision: u64) {
        self.update_open(path, text);
        self.records.get_mut(path).unwrap().open_revision = Some(revision);
    }
    pub fn update_open(&mut self, path: &Path, text: &str) {
        let stamp = self
            .records
            .get(path)
            .map(|r| r.stamp)
            .unwrap_or((SystemTime::UNIX_EPOCH, 0));
        let fingerprint = self
            .records
            .get(path)
            .map(|r| r.fingerprint)
            .unwrap_or_else(|| fingerprint(text));
        self.records.insert(
            path.into(),
            Record {
                stamp,
                links: scan(text),
                open_revision: None,
                fingerprint,
            },
        );
    }
    /// Refresh once, then use reverse edges to touch only referring notes and
    /// notes inside moved folders. Unrelated large files are never reread.
    pub fn prepare(
        &mut self,
        root: &Path,
        moves: Vec<Move>,
        open: &HashMap<PathBuf, String>,
    ) -> (Rules, Vec<(PathBuf, Vec<Patch>)>) {
        self.refresh_as(root, &moves);
        for (path, text) in open {
            self.update_open(&original(path, &moves), text);
        }
        self.rebuild();
        let before = self.catalog.clone();
        let after = Arc::new(Catalog::new(
            root,
            before.notes.iter().map(|n| moved(&n.path, &moves)),
        ));
        let rules = Rules {
            before,
            after,
            moves,
        };
        let mut candidates = BTreeSet::new();
        for m in &rules.moves {
            for (target, from) in &self.incoming {
                if target.starts_with(&m.old) {
                    candidates.extend(from.iter().cloned());
                }
            }
            candidates.extend(
                self.records
                    .keys()
                    .filter(|p| p.starts_with(&m.old))
                    .cloned(),
            );
        }
        let repairs = candidates
            .into_iter()
            .filter_map(|path| {
                let patches = rules.patches(&path, &self.records.get(&path)?.links);
                (!patches.is_empty()).then(|| (moved(&path, &rules.moves), patches))
            })
            .collect();
        (rules, repairs)
    }
    /// Copies rebase their own outgoing links only. References elsewhere must
    /// continue pointing at the originals. Folder-internal assets follow the
    /// copied folder; assets outside it keep their original targets.
    pub fn prepare_copies(&mut self, root: &Path, copies: Vec<Move>) -> Vec<(PathBuf, Vec<Patch>)> {
        let paths = inventory(root);
        self.catalog = Arc::new(Catalog::new(root, paths.iter().cloned()));
        let copied_paths = paths
            .into_iter()
            .filter(|path| copies.iter().any(|c| path.starts_with(&c.new)))
            .collect::<Vec<_>>();
        self.refresh_paths(&copied_paths);
        let rules = self.copy_rules(&copies);
        self.records
            .iter()
            .filter(|(path, _)| rules.moves.iter().any(|c| path.starts_with(&c.new)))
            .filter_map(|(path, record)| {
                let from = original(path, &rules.moves);
                let patches = rules.patches(&from, &record.links);
                (!patches.is_empty()).then(|| (path.clone(), patches))
            })
            .collect()
    }
    pub fn copy_rules(&self, copies: &[Move]) -> Rules {
        let before = Arc::new(Catalog::new(
            &self.catalog.root,
            self.catalog
                .notes
                .iter()
                .filter(|n| !copies.iter().any(|c| n.path.starts_with(&c.new)))
                .map(|n| n.path.clone()),
        ));
        Rules {
            before,
            after: self.catalog.clone(),
            moves: copies.to_vec(),
        }
    }
    /// Re-index only explicit copied/rewritten paths, without reading unrelated
    /// large notes. Ordinary workspace refresh still discovers other changes.
    pub fn refresh_paths(&mut self, paths: &[PathBuf]) {
        for path in paths {
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            let Ok(time) = meta.modified() else { continue };
            let stamp = (time, meta.len());
            if self.records.get(path).is_some_and(|r| r.stamp == stamp) {
                continue;
            }
            if let Ok(record) = with_text(path, |text| make_record(stamp, text)) {
                self.records.insert(path.clone(), record);
                self.parsed_files += 1;
            }
        }
        self.rebuild();
    }
    pub fn finish_moves(&mut self, rules: &Rules, affected: &[PathBuf]) {
        self.records = std::mem::take(&mut self.records)
            .into_iter()
            .map(|(path, record)| (moved(&path, &rules.moves), record))
            .collect();
        // Force only notes whose destinations changed to be parsed on refresh.
        for path in affected {
            if let Some(record) = self.records.get_mut(path) {
                record.stamp = (SystemTime::UNIX_EPOCH, 0);
            }
        }
        self.catalog = rules.after.clone();
        self.rebuild();
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            catalog: self.catalog.clone(),
            backlinks: Arc::new(self.backlinks.clone()),
        }
    }
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub catalog: Arc<Catalog>,
    pub backlinks: Arc<HashMap<PathBuf, Vec<PathBuf>>>,
}

#[derive(Clone)]
struct FileInfo {
    stamp: (SystemTime, u64),
    identity: Option<(u64, u64)>,
}
fn fingerprint(text: &str) -> u128 {
    use std::hash::Hasher;
    let mut a = std::collections::hash_map::DefaultHasher::new();
    let mut b = std::collections::hash_map::DefaultHasher::new();
    a.write(text.as_bytes());
    b.write_u64(0x656e736f5f6c696e);
    b.write(text.as_bytes());
    (u128::from(a.finish()) << 64) | u128::from(b.finish())
}
fn make_record(stamp: (SystemTime, u64), text: &str) -> Record {
    Record {
        stamp,
        links: scan(text),
        open_revision: None,
        fingerprint: fingerprint(text),
    }
}
#[cfg(unix)]
pub(crate) fn file_identity(_: &Path, metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}
#[cfg(windows)]
pub(crate) fn file_identity(path: &Path, _: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let file = std::fs::File::open(path).ok()?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return None;
    }
    Some((
        info.dwVolumeSerialNumber as u64,
        ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    ))
}
#[cfg(not(any(unix, windows)))]
pub(crate) fn file_identity(_: &Path, _: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

pub struct Repair {
    pub path: PathBuf,
    pub patches: Vec<Patch>,
    pub rules: Rules,
}

/// Filesystem observation is the common entry point for every arrival route.
/// Assets need only identity/metadata; note contents are read only on change.
#[derive(Default)]
pub struct Changes {
    pub moves: Vec<Move>,
    pub external_moves: Vec<Move>,
    pub copies: Vec<Move>,
    pub changed_notes: Vec<PathBuf>,
}
impl Index {
    pub(crate) fn path_for_identity(&self, identity: (u64, u64)) -> Option<&PathBuf> {
        let mut paths = self.files.iter().filter(|(_, file)| file.identity == Some(identity)).map(|(path, _)| path);
        let first = paths.next()?;
        paths.next().is_none().then_some(first)
    }

    pub fn observe(&mut self, root: &Path, explicit_moves: Vec<Move>) -> Changes {
        let mut current = HashMap::new();
        let mut dirs = vec![root.to_owned()];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let name = entry.file_name();
                let hidden = name.to_string_lossy().starts_with('.');
                if kind.is_symlink() || (hidden && !(dir == root && name == ".assets")) {
                    continue;
                }
                let path = entry.path();
                if kind.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if !kind.is_file() {
                    continue;
                }
                let Ok(meta) = entry.metadata() else { continue };
                let Ok(time) = meta.modified() else { continue };
                let stamp = (time, meta.len());
                let identity = self
                    .files
                    .get(&path)
                    .filter(|old| old.stamp == stamp)
                    .and_then(|old| old.identity)
                    .or_else(|| file_identity(&path, &meta));
                current.insert(path, FileInfo { stamp, identity });
            }
        }
        let mut changes = Changes {
            moves: explicit_moves,
            ..Default::default()
        };
        // An identity is accepted only for a single vanished path and a single
        // arriving path: hard links and ambiguous matches cannot redirect links.
        let mut gone = HashMap::<(u64, u64), Vec<PathBuf>>::new();
        let mut arrived = HashMap::<(u64, u64), Vec<PathBuf>>::new();
        for (path, info) in &self.files {
            if !current.contains_key(path)
                && let Some(id) = info.identity
            {
                gone.entry(id).or_default().push(path.clone());
            }
        }
        for (path, info) in &current {
            if !self.files.contains_key(path)
                && let Some(id) = info.identity
            {
                arrived.entry(id).or_default().push(path.clone());
            }
        }
        for (identity, old) in gone {
            if old.len() != 1 {
                continue;
            }
            let Some(new) = arrived.get(&identity).filter(|v| v.len() == 1) else {
                continue;
            };
            if moved(&old[0], &changes.moves) != new[0] {
                let relocation = Move {
                    old: old[0].clone(),
                    new: new[0].clone(),
                };
                changes.external_moves.push(relocation.clone());
                changes.moves.push(relocation);
            }
        }
        self.staged.clear();
        for (path, info) in &current {
            if !crate::files::is_markdown(path)
                || path.strip_prefix(root).ok().is_some_and(|p| {
                    p.components()
                        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                })
            {
                continue;
            }
            let from = original(path, &changes.moves);
            if self
                .records
                .get(&from)
                .is_some_and(|r| r.stamp == info.stamp)
            {
                continue;
            }
            if let Ok(record) = with_text(path, |text| make_record(info.stamp, text)) {
                self.staged.insert(path.clone(), record);
                changes.changed_notes.push(path.clone());
            }
        }
        if changes.changed_notes.is_empty() {
            self.files = current;
            return changes;
        }
        // Equal contents identify copies, including on first opening a workspace.
        // Prefer an origin that actually resolves the note's local assets.
        let mut by_hash = HashMap::<u128, Vec<PathBuf>>::new();
        for (path, record) in self.records.iter().chain(self.staged.iter()) {
            let options = by_hash.entry(record.fingerprint).or_default();
            if !options.contains(path) {
                options.push(path.clone());
            }
        }
        for path in &changes.changed_notes {
            if self.files.contains_key(path)
                || changes.moves.iter().any(|m| path.starts_with(&m.new))
            {
                continue;
            }
            let record = &self.staged[path];
            let Some(options) = by_hash.get(&record.fingerprint) else {
                continue;
            };
            let score = |from: &Path| {
                record
                    .links
                    .iter()
                    .filter(|l| l.kind != Kind::Wiki)
                    .filter_map(|l| local_target(root, from, &l.value))
                    .filter(|p| current.contains_key(p))
                    .count()
            };
            let here = score(path);
            let mut origins = options
                .iter()
                .filter(|p| *p != path)
                .filter(|p| score(p) > here)
                .map(|p| (score(p), p.clone()))
                .collect::<Vec<_>>();
            origins.sort_by(|a, b| b.0.cmp(&a.0));
            if let Some((best, origin)) = origins.first()
                && origins.iter().filter(|(s, _)| s == best).count() == 1
            {
                changes.copies.push(Move {
                    old: origin.clone(),
                    new: path.clone(),
                });
            }
        }
        self.files = current;
        changes
    }

    /// A synced/imported note may have no known original. Repair only broken
    /// local paths with one exact match in its workspace ancestors.
    pub fn recover_paths(&self, root: &Path, paths: &[PathBuf]) -> Vec<Repair> {
        let mut result = Vec::new();
        for path in paths {
            let Some(record) = self.records.get(path) else {
                continue;
            };
            let mut patches = Vec::new();
            let mut mappings = Vec::new();
            for link in &record.links {
                if link.kind == Kind::Wiki {
                    continue;
                }
                let Some(target) = local_target(root, path, &link.value) else {
                    continue;
                };
                if self.files.contains_key(&target) {
                    continue;
                }
                let raw = decode(link.value.split(['#', '?']).next().unwrap_or(""));
                if raw.starts_with('/') || raw.contains(':') {
                    continue;
                }
                let candidates = path
                    .parent()
                    .into_iter()
                    .flat_map(|p| p.ancestors())
                    .take_while(|p| p.starts_with(root))
                    .map(|base| normalize(&base.join(&raw)))
                    .filter(|p| p.starts_with(root) && self.files.contains_key(p))
                    .collect::<BTreeSet<_>>();
                if candidates.len() != 1 {
                    continue;
                }
                let rules = Rules {
                    before: self.catalog.clone(),
                    after: self.catalog.clone(),
                    moves: vec![Move {
                        old: target,
                        new: candidates.into_iter().next().unwrap(),
                    }],
                };
                patches.extend(rules.patches(path, std::slice::from_ref(link)));
                mappings.extend(rules.moves);
            }
            if !patches.is_empty() {
                result.push(Repair {
                    path: path.clone(),
                    patches,
                    rules: Rules {
                        before: self.catalog.clone(),
                        after: self.catalog.clone(),
                        moves: mappings,
                    },
                });
            }
        }
        result
    }
}
/// Write a repaired closed note without allocating a second large document.
/// The existing save helper checks for concurrent writes and replaces atomically.
pub fn write_patches(path: &Path, patches: &[Patch]) -> std::io::Result<()> {
    use std::io::Write;
    let stamp = std::fs::metadata(path)?;
    let temp = path.with_file_name(format!(
        ".enso-links-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = with_text(path, |text| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let mut at = 0;
        for p in patches {
            if p.range.start < at || text.get(p.range.clone()) != Some(p.before.as_str()) {
                return Err(std::io::Error::other(
                    "A linked note changed while its paths were being updated.",
                ));
            }
            file.write_all(text[at..p.range.start].as_bytes())?;
            file.write_all(p.after.as_bytes())?;
            at = p.range.end;
        }
        file.write_all(text[at..].as_bytes())?;
        file.sync_all()?;
        let now = std::fs::metadata(path)?;
        if now.modified()? != stamp.modified()? || now.len() != stamp.len() {
            return Err(std::io::Error::other(
                "A linked note changed on disk; its links were not overwritten.",
            ));
        }
        std::fs::set_permissions(&temp, stamp.permissions())?;
        Ok(())
    })
    .and_then(|r| r)
    .and_then(|_| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

pub fn shift_offset(at: usize, patches: &[Patch]) -> usize {
    let mut delta: isize = 0;
    for p in patches {
        if at < p.range.start {
            break;
        }
        if at < p.range.end {
            return (p.range.start as isize + delta + p.after.len() as isize).max(0) as usize;
        }
        delta += p.after.len() as isize - p.range.len() as isize;
    }
    (at as isize + delta).max(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    struct Workspace(PathBuf);
    impl Workspace {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "enso-links-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn put(&self, path: &str, text: &str) -> PathBuf {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }
    }
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn scanner_keeps_precise_spans_and_ignores_code() {
        let text = "[[नोट|alias]] [go](notes/a%20b.md#intro \"title\")\n<img src=\".assets/a.png\" alt=\"x\">\n[ref]: ../photo.png \"title\"\n`[[code]]`\n```md\n[[fence]] ![](fake.png)\n```\n    [[indented]]\n\\[[escaped]]\n<!-- [[comment]] -->\n[[real]]";
        let links = scan(text);
        assert_eq!(
            links.iter().map(|l| l.value.as_str()).collect::<Vec<_>>(),
            [
                "नोट",
                "notes/a%20b.md#intro",
                ".assets/a.png",
                "../photo.png",
                "real"
            ]
        );
        for link in links {
            assert_eq!(&text[link.range], link.value);
        }
        assert!(scan("[[".repeat(100_000).as_str()).is_empty());
        assert_eq!(decode("a%20b/%é"), "a b/%é");
    }
    #[test]
    fn ambiguous_names_need_paths_and_aliases_keep_their_labels() {
        let ws = Workspace::new();
        let a = ws.put("one/Note.md", "");
        let b = ws.put("two/Note.md", "");
        let source = ws.put("Start.md", "");
        let c = Catalog::new(&ws.0, [a.clone(), b.clone(), source.clone()]);
        assert_eq!(c.resolve(&source, "Note"), None);
        assert_eq!(c.resolve(&source, "one/Note#intro"), Some(a.clone()));
        assert_eq!(c.resolve(&a, "Note"), Some(a.clone()));
        assert_eq!(c.destination(&source, &b), "two/Note.md");
        assert_eq!(c.resolve(&source, "one/Note.md"), Some(a.clone()));
        let single = Catalog::new(&ws.0, [b.clone(), source.clone()]);
        assert_eq!(single.destination(&source, &b), "Note.md");
        assert_eq!(single.resolve(&source, "Note.md"), Some(b));
        assert_eq!(c.suggestions(&source, "not").len(), 2);
    }
    #[test]
    fn renaming_updates_wiki_markdown_reference_and_html_destinations() {
        let ws = Workspace::new();
        let target = ws.put("Old.md", "");
        let from=ws.put("From.md","[[Old|Keep this label]] and [regular](Old.md#intro \"title\")\n[ref]: Old.md\n<a href=\"Old.md\">old</a>\n`[[Old]]`");
        let mut index = Index::default();
        index.refresh(&ws.0);
        assert_eq!(index.backlinks.get(&target), Some(&vec![from.clone()]));
        let new = ws.0.join("New.md");
        std::fs::rename(&target, &new).unwrap();
        let (rules, repairs) = index.prepare(
            &ws.0,
            vec![Move {
                old: target,
                new: new.clone(),
            }],
            &HashMap::new(),
        );
        assert_eq!(repairs.len(), 1);
        write_patches(&repairs[0].0, &repairs[0].1).unwrap();
        let result = std::fs::read_to_string(&from).unwrap();
        assert_eq!(
            result,
            "[[New|Keep this label]] and [regular](New.md#intro \"title\")\n[ref]: New.md\n<a href=\"New.md\">old</a>\n`[[Old]]`"
        );
        index.finish_moves(&rules, &[from.clone()]);
        index.refresh(&ws.0);
        assert_eq!(index.backlinks.get(&new), Some(&vec![from]));
    }
    #[test]
    fn moving_a_folder_rebases_outgoing_links_and_incoming_asset_links() {
        let ws = Workspace::new();
        let note = ws.put(
            "old/Note.md",
            "<img src=\"../.assets/images/photo%20one.png\" width=\"50%\" />\n[other](../Other.md)",
        );
        ws.put("Other.md", "");
        ws.put(".assets/images/photo one.png", "image");
        let from = ws.put("From.md", "[[old/Note]] <img src=\"old/pic.png\" />");
        ws.put("old/pic.png", "image");
        let mut index = Index::default();
        index.refresh(&ws.0);
        std::fs::create_dir_all(ws.0.join("nested")).unwrap();
        let old = ws.0.join("old");
        let new = ws.0.join("nested/new");
        std::fs::rename(&old, &new).unwrap();
        let (_, repairs) = index.prepare(
            &ws.0,
            vec![Move {
                old,
                new: new.clone(),
            }],
            &HashMap::new(),
        );
        for (path, patches) in repairs {
            write_patches(&path, &patches).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(new.join("Note.md")).unwrap(),
            "<img src=\"../../.assets/images/photo%20one.png\" width=\"50%\" />\n[other](../../Other.md)"
        );
        assert_eq!(
            std::fs::read_to_string(from).unwrap(),
            "[[Note]] <img src=\"nested/new/pic.png\" />"
        );
        assert!(!note.exists());
    }
    #[test]
    fn explicit_extensions_stay_explicit_on_rename_and_paths_do_not_shadow() {
        let ws = Workspace::new();
        let old = ws.put("big.md", "");
        let from = ws.put("sub/Start.md", "[[big.md#Intro|Original label]]");
        let destination = ws.put("docs/big.md", "");
        let shadow = ws.put("sub/docs/big.md", "");
        let catalog = Catalog::new(
            &ws.0,
            [old.clone(), from.clone(), destination.clone(), shadow],
        );
        let explicit = catalog.destination(&from, &destination);
        assert_eq!(catalog.resolve(&from, &explicit), Some(destination));
        let mut index = Index::default();
        index.refresh(&ws.0);
        let new = ws.0.join("renamed.md");
        std::fs::rename(&old, &new).unwrap();
        let (_, repairs) = index.prepare(&ws.0, vec![Move { old, new }], &HashMap::new());
        for (path, patches) in repairs {
            write_patches(&path, &patches).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(from).unwrap(),
            "[[renamed.md#Intro|Original label]]"
        );
    }
    #[test]
    fn copies_rebase_outgoing_paths_without_changing_originals_or_their_backlinks() {
        let ws = Workspace::new();
        let source = r#"<img src=".assets/images/photo.jpg" alt="photo" width="36.6%" style="display: block; margin-left: auto; margin-right: auto;" />
<img src=".assets/images/compressO-IMG_4116 (2).jpg" alt="Half width" width="24.6%" />
[tasks](tasks.md)
[^1]: The footnote's text.
[^note]: Footnotes can have **formatting** too.
<img src="https://example.com/image.png" />"#;
        let old = ws.put("markdown-features 2.md", source);
        let incoming = ws.put("Incoming.md", "[original](markdown-features%202.md)");
        ws.put("tasks.md", "Tasks");
        ws.put(".assets/images/photo.jpg", "image");
        std::fs::create_dir(ws.0.join("lol")).unwrap();
        let mut index = Index::default();
        index.refresh(&ws.0);
        let reads = index.parsed_files;
        let new = crate::files::copy_into(&old, &ws.0.join("lol")).unwrap();
        let repairs = index.prepare_copies(
            &ws.0,
            vec![Move {
                old: old.clone(),
                new: new.clone(),
            }],
        );
        assert_eq!(repairs.len(), 1);
        assert_eq!(index.parsed_files, reads + 1, "only the new copy is parsed");
        assert_eq!(repairs[0].0, new);
        write_patches(&repairs[0].0, &repairs[0].1).unwrap();
        assert_eq!(
            std::fs::read_to_string(&new).unwrap(),
            source
                .replace(
                    "src=\".assets/images/photo.jpg",
                    "src=\"../.assets/images/photo.jpg"
                )
                .replace(
                    "src=\".assets/images/compressO-IMG_4116 (2).jpg",
                    "src=\"../.assets/images/compressO-IMG_4116%20%282%29.jpg"
                )
                .replace("(tasks.md)", "(../tasks.md)")
        );
        assert_eq!(std::fs::read_to_string(&old).unwrap(), source);
        assert_eq!(
            std::fs::read_to_string(&incoming).unwrap(),
            "[original](markdown-features%202.md)"
        );
        index.refresh(&ws.0);
        assert_eq!(index.backlinks.get(&old), Some(&vec![incoming]));
    }
    #[test]
    fn copying_a_folder_keeps_its_internal_image_paths_relative() {
        let ws = Workspace::new();
        let source = "![inside](images/photo.jpg)\n![outside](../.assets/photo.jpg)";
        ws.put("original/Note.md", source);
        ws.put("original/images/photo.jpg", "image");
        ws.put(".assets/photo.jpg", "image");
        std::fs::create_dir(ws.0.join("lol")).unwrap();
        let old = ws.0.join("original");
        let new = crate::files::copy_into(&old, &ws.0.join("lol")).unwrap();
        let mut index = Index::default();
        let repairs = index.prepare_copies(
            &ws.0,
            vec![Move {
                old,
                new: new.clone(),
            }],
        );
        for (path, patches) in repairs {
            write_patches(&path, &patches).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(new.join("Note.md")).unwrap(),
            "![inside](images/photo.jpg)\n![outside](../../.assets/photo.jpg)"
        );
    }
    #[test]
    fn moving_a_root_note_into_lol_rebases_its_hidden_asset_path() {
        let ws = Workspace::new();
        let source = r#"<img src=".assets/images/photo.jpg" alt="photo" width="36.6%" style="display: block; margin-left: auto; margin-right: auto;" />"#;
        let old = ws.put("big.md", source);
        ws.put(".assets/images/photo.jpg", "image");
        std::fs::create_dir(ws.0.join("lol")).unwrap();
        let new = ws.0.join("lol/big.md");
        let mut index = Index::default();
        index.refresh(&ws.0);
        std::fs::rename(&old, &new).unwrap();
        let (_, repairs) = index.prepare(
            &ws.0,
            vec![Move {
                old,
                new: new.clone(),
            }],
            &HashMap::new(),
        );
        assert_eq!(repairs.len(), 1);
        write_patches(&repairs[0].0, &repairs[0].1).unwrap();
        assert_eq!(
            std::fs::read_to_string(&new).unwrap(),
            source.replace(".assets/images/photo.jpg", "../.assets/images/photo.jpg")
        );
    }
    #[test]
    fn asset_moves_touch_only_referrers_and_unchanged_large_notes_are_not_parsed_again() {
        let ws = Workspace::new();
        let image = ws.put("photo.png", "image");
        let from = ws.put("note.md", "![Photo](photo.png)\n<img src='photo.png' />");
        ws.put(
            "big.md",
            &"some plain text without a link\n".repeat(200_000),
        );
        let mut index = Index::default();
        assert!(index.refresh(&ws.0));
        let reads = index.parsed_files;
        assert!(!index.refresh(&ws.0));
        assert_eq!(index.parsed_files, reads);
        std::fs::create_dir(ws.0.join("images")).unwrap();
        let target = ws.0.join("images/photo.png");
        std::fs::rename(&image, &target).unwrap();
        let (rules, repairs) = index.prepare(
            &ws.0,
            vec![Move {
                old: image,
                new: target,
            }],
            &HashMap::new(),
        );
        assert_eq!(repairs.len(), 1);
        assert_eq!(index.parsed_files, reads);
        write_patches(&repairs[0].0, &repairs[0].1).unwrap();
        index.finish_moves(&rules, &[from]);
        index.refresh(&ws.0);
        assert_eq!(
            index.parsed_files,
            reads + 1,
            "only the rewritten referrer is re-indexed"
        );
    }
    #[test]
    fn changed_destinations_never_overwrite_other_edits() {
        let ws = Workspace::new();
        let path = ws.put("note.md", "![x](a.png)");
        let range = scan("![x](a.png)")[0].range.clone();
        std::fs::write(&path, "Other text that moved the destination.").unwrap();
        assert!(
            write_patches(
                &path,
                &[Patch {
                    range,
                    before: "a.png".into(),
                    after: "b.png".into()
                }]
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "Other text that moved the destination."
        );
    }
}
