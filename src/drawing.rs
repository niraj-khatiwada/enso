//! Drawings: `.excalidraw` files, opened in a window of their own with the
//! Excalidraw editor (crates/excalidraw) and saved as you draw. The file is
//! Excalidraw's own format, so it opens in Excalidraw and Obsidian too.

use gpui_excalidraw::{Editor, Theme, ui::icons::IconAssets};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AnyWindowHandle, App, AppContext, AssetSource, Global, SharedString, Task,
    WindowOptions,
};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

/// How long drawing pauses before the file is written.
const SAVE_DELAY: Duration = Duration::from_millis(800);

pub fn is_drawing(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("excalidraw"))
}

/// The app's assets with the drawing editor's icons in front of them.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        IconAssets.load_with(path, |p| gpui_kit::assets::AllAssets.load(p))
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        IconAssets.list_with(path, |p| gpui_kit::assets::AllAssets.list(p))
    }
}

/// Open drawing windows by file, so opening one again brings its window up.
#[derive(Default)]
struct Windows(HashMap<PathBuf, AnyWindowHandle>);

impl Global for Windows {}

pub fn init(cx: &mut App) {
    gpui_excalidraw::ui::fonts::register_hand_drawn_fonts(cx);
    cx.set_global(Windows::default());
}

/// A new, empty drawing in `dir`: "Drawing.excalidraw", then "Drawing 2…".
pub fn create(dir: &Path) -> std::io::Result<PathBuf> {
    let path = (1..)
        .map(|n| match n {
            1 => dir.join("Drawing.excalidraw"),
            n => dir.join(format!("Drawing {n}.excalidraw")),
        })
        .find(|p| !p.exists())
        .expect("a free name");
    write_empty(&path)?;
    Ok(path)
}

/// A drawing for a note, kept with its images: `root/.assets/drawings`,
/// named like pasted images so the note's link needs no escaping.
pub fn create_for_note(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join(DRAWINGS_DIR);
    std::fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = (1..)
        .map(|n| match n {
            1 => dir.join(format!("drawing-{stamp}.excalidraw")),
            n => dir.join(format!("drawing-{stamp}-{n}.excalidraw")),
        })
        .find(|p| !p.exists())
        .expect("a free name");
    write_empty(&path)?;
    Ok(path)
}

pub const DRAWINGS_DIR: &str = ".assets/drawings";

fn write_empty(path: &Path) -> std::io::Result<()> {
    let json = Editor::new().scene_json().map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Open `path` in its drawing window, or bring that window to the front.
pub fn open(path: PathBuf, cx: &mut App) -> Result<(), String> {
    if let Some(handle) = cx.try_global::<Windows>().and_then(|w| w.0.get(&path).copied())
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return Ok(());
    }
    let mut editor = Editor::new();
    // Excalidraw.com's welcome talks about browser storage; here it's a file.
    editor.welcome = false;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if !text.trim().is_empty() {
        editor.load_scene_json(&text)?;
    }
    // The page follows enso's appearance, whatever the file last had.
    editor.set_theme(if cx.theme().mode.is_dark() { Theme::Dark } else { Theme::Light });
    let saved = Rc::new(RefCell::new(editor.scene_json().unwrap_or_default()));
    let title = path
        .file_stem()
        .map(|n| format!("{} — enso", n.to_string_lossy()))
        .unwrap_or_else(|| "Drawing — enso".into());
    let file = path.clone();
    let handle = gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
        window.set_window_title(&title);
        let editor = cx.new(|_| editor);
        // Write after each pause in drawing, and once more on close.
        let pending: Rc<RefCell<Option<Task<()>>>> = Rc::default();
        cx.observe(&editor, {
            let (file, saved, pending) = (file.clone(), saved.clone(), pending.clone());
            move |editor, cx| {
                let (file, saved, weak) = (file.clone(), saved.clone(), editor.downgrade());
                *pending.borrow_mut() = Some(cx.spawn(async move |cx| {
                    cx.background_executor().timer(SAVE_DELAY).await;
                    let wrote = weak.update(cx, |editor, _| save(editor, &file, &saved));
                    // Notes that show this drawing draw its new version.
                    if wrote.unwrap_or(false) {
                        cx.update(|cx| cx.refresh_windows());
                    }
                }));
            }
        })
        .detach();
        window.on_window_should_close(cx, {
            let weak = editor.downgrade();
            move |_, cx| {
                if weak.update(cx, |editor, _| save(editor, &file, &saved)).unwrap_or(false) {
                    cx.refresh_windows();
                }
                true
            }
        });
        editor
    })
    .map_err(|e| e.to_string())?;
    cx.global_mut::<Windows>().0.insert(path, handle.0);
    Ok(())
}

/// A drawing as a picture for notes and exports: a PNG made by the drawing's
/// own renderer (hand-drawn fonts included), at `SCALE` times its size.
pub struct Picture {
    pub image: Arc<gpui_kit::Image>,
    /// In CSS pixels.
    pub width: f32,
    pub height: f32,
}

/// Pictures are this many pixels per CSS pixel: sharp on Retina and in print.
const SCALE: f32 = 3.;

/// `path` drawn for a note, light or dark, on a clear background. Made once
/// per version of the file: a changed drawing is drawn again.
pub fn picture(path: &Path, dark: bool) -> Option<Arc<Picture>> {
    static CACHE: Mutex<Option<HashMap<(PathBuf, bool), (SystemTime, Arc<Picture>)>>> = Mutex::new(None);
    let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let key = (path.to_path_buf(), dark);
    if let Some((at, picture)) = CACHE.lock().ok()?.get_or_insert_default().get(&key)
        && *at == stamp
    {
        return Some(picture.clone());
    }
    let text = std::fs::read_to_string(path).ok()?;
    let (scene, state) = gpui_excalidraw::json_to_scene(&text).ok()?;
    // Nothing drawn yet: the note shows a place to start instead.
    if scene.is_empty() {
        return None;
    }
    let png = gpui_excalidraw::export_png_with_dark_mode(&scene, "transparent", SCALE, &state.files, dark).ok()?;
    // The PNG header's width and height (IHDR).
    let size = |at: usize| png.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32);
    let (width, height) = (size(16)? / SCALE, size(20)? / SCALE);
    let image = Arc::new(gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, png.clone()));
    let picture = Arc::new(Picture { image, width, height });
    let mut cache = CACHE.lock().ok()?;
    let cache = cache.get_or_insert_default();
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert(key, (stamp, picture.clone()));
    Some(picture)
}

/// Write the drawing if it changed since it was last written. A pointer still
/// down (mid-stroke, mid-drag) waits for the next pause.
fn save(editor: &mut Editor, file: &Path, saved: &RefCell<String>) -> bool {
    if editor.is_interacting() {
        return false;
    }
    let Ok(json) = editor.scene_json() else { return false };
    if *saved.borrow() == json {
        return false;
    }
    if std::fs::write(file, &json).is_err() {
        return false;
    }
    *saved.borrow_mut() = json;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_drawings_get_free_names_and_load_back() {
        let dir = std::env::temp_dir().join(format!("enso-drawing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = create(&dir).unwrap();
        let second = create(&dir).unwrap();
        assert_eq!(first.file_name().unwrap(), "Drawing.excalidraw");
        assert_eq!(second.file_name().unwrap(), "Drawing 2.excalidraw");
        assert!(is_drawing(&first) && !is_drawing(&dir.join("note.md")));
        // Excalidraw's own envelope, which the editor reads back.
        let text = std::fs::read_to_string(&first).unwrap();
        assert!(text.contains("\"type\": \"excalidraw\"") || text.contains("\"type\":\"excalidraw\""));
        Editor::new().load_scene_json(&text).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_drawing_becomes_a_picture_that_follows_the_file() {
        let root = std::env::temp_dir().join(format!("enso-drawing-pic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = create_for_note(&root).unwrap();
        assert!(path.starts_with(root.join(DRAWINGS_DIR)));
        assert!(!path.to_string_lossy().contains(' '));
        let mut editor = Editor::new();
        editor.add_demo_scene();
        std::fs::write(&path, editor.scene_json().unwrap()).unwrap();
        let light = picture(&path, false).unwrap();
        assert!(light.width > 10. && light.height > 10.);
        // The same version is drawn once; dark mode is its own picture.
        assert!(Arc::ptr_eq(&light, &picture(&path, false).unwrap()));
        assert!(!Arc::ptr_eq(&light, &picture(&path, true).unwrap()));
        // A newer file is drawn again.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, Editor::new().scene_json().unwrap().replace("\"elements\": []", "\"elements\":[]")).unwrap();
        let file = std::fs::File::options().append(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        let _ = picture(&path, false).map(|p| assert!(!Arc::ptr_eq(&light, &p)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saving_writes_only_when_the_drawing_changed() {
        let dir = std::env::temp_dir().join(format!("enso-drawing-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Sketch.excalidraw");
        let mut editor = Editor::new();
        let saved = RefCell::new(editor.scene_json().unwrap());
        save(&mut editor, &file, &saved);
        assert!(!file.exists());
        editor.add_demo_scene();
        save(&mut editor, &file, &saved);
        let mut reopened = Editor::new();
        reopened.load_scene_json(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(reopened.scene_json().unwrap(), editor.scene_json().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
