use std::{
    fs, io,
    path::{Path, PathBuf},
};

const SAMPLE: &str = include_str!("../Get Started.md");

/// The sample is a normal user-owned file. Its marker is separate so deleting
/// or renaming the note does not cause it to be recreated on every launch.
pub fn first_launch(directory: &Path) -> io::Result<Option<(PathBuf, String)>> {
    let marker = directory.join(".first-launch-complete");
    if marker.try_exists()? {
        return Ok(None);
    }
    fs::create_dir_all(directory)?;
    let path = directory.join("Get Started.md");
    match crate::files::save_as(&path, SAMPLE) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    // Never replace a guide the user has edited, including after a failed start.
    let source = crate::files::read_text(&path)?;
    match crate::files::save_as(&marker, "1\n") {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        Err(error) => return Err(error),
    }
    Ok(Some((path, source)))
}

pub fn load() -> io::Result<Option<(PathBuf, String)>> {
    let directory = dirs::data_local_dir()
        .map(crate::files::app_folder)
        .ok_or_else(|| io::Error::other("Unable to locate enso's application data folder"))?;
    first_launch(&directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_opens_once_and_preserves_user_edits() {
        let directory =
            std::env::temp_dir().join(format!("enso-onboarding-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let (path, source) = first_launch(&directory).unwrap().unwrap();
        assert_eq!(path.file_name().unwrap(), "Get Started.md");
        assert_eq!(source, SAMPLE);
        crate::files::save(&path, &source, "My edited guide").unwrap();
        assert!(first_launch(&directory).unwrap().is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), "My edited guide");
        // Retrying an interrupted first launch also preserves existing content.
        fs::remove_file(directory.join(".first-launch-complete")).unwrap();
        assert_eq!(
            first_launch(&directory).unwrap().unwrap().1,
            "My edited guide"
        );
        fs::remove_file(path).unwrap();
        assert!(first_launch(&directory).unwrap().is_none());
        fs::remove_dir_all(directory).unwrap();
    }
}
