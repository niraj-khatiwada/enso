//! The update notice: at most once a day, on launch, ask GitHub for enso's
//! latest release and say so when it's newer than this build. Nothing is
//! downloaded or installed; the toast's button opens the release page.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LATEST: &str = "https://api.github.com/repos/niraj-khatiwada/enso/releases/latest";
const RELEASES: &str = "https://github.com/niraj-khatiwada/enso/releases/tag/";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// A release newer than this build.
pub struct Release {
    /// Its version, without the tag's "v": "1.2.0".
    pub version: String,
    /// Its page on GitHub.
    pub url: String,
}

/// When enso last asked GitHub (seconds since 1970), in its config folder.
fn stamp_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("update-checked"))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether a day has passed since the last check (or there never was one).
pub fn due() -> bool {
    let Some(path) = stamp_path() else { return false };
    let last = std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse::<u64>().ok());
    last.is_none_or(|last| now().saturating_sub(last) >= DAY.as_secs())
}

/// Ask GitHub for the latest release (blocking: run it in the background).
/// `None` when this build is current or anything went wrong; a failed check
/// is tried again on the next launch.
pub fn check() -> Option<Release> {
    check_with_status().ok().flatten()
}

/// A manual check distinguishes a current build from a failed request.
pub fn check_with_status() -> Result<Option<Release>, String> {
    let reply = crate::http::get_text(LATEST, "application/vnd.github+json")
        .map_err(|_| "Could not check for updates. Please try again.".to_string())?;
    let tag = tag_name(&reply).filter(|tag| parse(tag).is_some())
        .ok_or_else(|| "The update service returned an unrecognized version.".to_string())?;
    if let Some(path) = stamp_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, now().to_string());
    }
    Ok(newer(&tag, env!("CARGO_PKG_VERSION")).then(|| Release {
        version: tag.trim_start_matches('v').to_string(),
        url: format!("{RELEASES}{tag}"),
    }))
}

/// The `"tag_name"` of a GitHub release reply, without pulling in a JSON
/// parser for one string. Tags never hold quotes or escapes.
fn tag_name(json: &str) -> Option<String> {
    let rest = &json[json.find("\"tag_name\"")? + "\"tag_name\"".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let tag = &rest[..rest.find('"')?];
    (!tag.is_empty() && !tag.contains('\\')).then(|| tag.to_string())
}

/// "v1.2.3" or "1.2.3" as numbers. A pre-release ("1.2.3-beta.1") counts as
/// its release.
fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    parts.next().is_none().then_some((major, minor, patch))
}

/// Whether the release tagged `latest` is newer than version `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{newer, tag_name};

    #[test]
    fn compares_versions_by_number() {
        assert!(newer("v1.0.1", "1.0.0"));
        assert!(newer("v1.1.0", "1.0.9"));
        assert!(newer("v2.0.0", "1.12.3"));
        assert!(newer("v1.10.0", "1.9.0"));
        assert!(newer("1.0.1", "1.0.0"));
        assert!(!newer("v1.0.0", "1.0.0"));
        assert!(!newer("v0.9.9", "1.0.0"));
        assert!(!newer("v1.0.0-beta.1", "1.0.0"));
        assert!(!newer("nightly", "1.0.0"));
        assert!(!newer("v1.0.0.1", "1.0.0"));
        assert!(!newer("", "1.0.0"));
    }

    #[test]
    fn reads_the_tag_from_a_release_reply() {
        let reply = r#"{"url":"x","html_url":"https://github.com/a","tag_name": "v1.2.0","name":"enso 1.2.0"}"#;
        assert_eq!(tag_name(reply).as_deref(), Some("v1.2.0"));
        assert_eq!(tag_name(r#"{"message":"Not Found"}"#), None);
        assert_eq!(tag_name(r#"{"tag_name":null}"#), None);
    }
}
