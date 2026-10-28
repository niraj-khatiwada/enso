use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

pub fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "md" | "markdown" | "mdown" | "mkd" | "mdx"
        )
    })
}

pub fn read_text(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "This appears to be a binary file. Enso opens UTF-8 text files.",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "This file is not UTF-8 text."))
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}

pub fn list_folder(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        // Do not follow symlinks into another workspace or cycles.
        if kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
            continue;
        }
        entries.push(Entry {
            path: entry.path(),
            directory: kind.is_dir(),
        });
    }
    entries.sort_by_key(|e| {
        (
            !e.directory,
            e.path.file_name().unwrap_or_default().to_ascii_lowercase(),
        )
    });
    Ok(entries)
}

pub fn create_file(root: &Path, name: &str) -> io::Result<PathBuf> {
    let name = name.trim();
    let relative = Path::new(name);
    if name.is_empty()
        || relative.components().count() != 1
        || !matches!(relative.components().next(), Some(Component::Normal(_)))
        || name.contains(['/', '\\', ':'])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a filename without folders, such as Notes.md.",
        ));
    }
    let path = root.join(relative);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(path)
}

/// Detect changes by another application before writing. Keep the original file
/// intact until a complete replacement has been written beside it.
pub fn save(path: &Path, expected: &str, content: &str) -> io::Result<()> {
    if read_text(path)? != expected {
        return Err(io::Error::other(
            "The file changed on disk. Automatic saving stopped to preserve both versions; your edits remain open in Enso.",
        ));
    }
    replace(path, content)
}

pub fn save_as(path: &Path, content: &str) -> io::Result<()> {
    // A new path must never overwrite another document accidentally.
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()
}

fn replace(path: &Path, content: &str) -> io::Result<()> {
    let temp = path.with_file_name(format!(
        ".enso-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temp, fs::metadata(path)?.permissions())?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_lifecycle_guards_existing_data() {
        let root = std::env::temp_dir().join(format!("enso-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(create_file(&root, "../escape.md").is_err());
        assert!(create_file(&root, "a\\b.md").is_err());
        let path = create_file(&root, "note.md").unwrap();
        assert!(create_file(&root, "note.md").is_err());
        save(&path, "", "# Hello\nनमस्ते").unwrap();
        assert_eq!(read_text(&path).unwrap(), "# Hello\nनमस्ते");
        fs::write(&path, "external edit").unwrap();
        assert!(save(&path, "# Hello\nनमस्ते", "my edit").is_err());
        assert!(save_as(&path, "overwrite").is_err());
        assert_eq!(read_text(&path).unwrap(), "external edit");
        fs::write(root.join("binary"), [0, 1, 2]).unwrap();
        assert!(read_text(&root.join("binary")).is_err());
        assert!(is_markdown(Path::new("NOTE.MD")));
        fs::remove_dir_all(root).unwrap();
    }
}
