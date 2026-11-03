use std::borrow::Cow;

use gpui_kit::*;

include!(concat!(env!("OUT_DIR"), "/icons_generated.rs"));

const PREFIX: &str = "icons/";

#[derive(Clone, Copy, Debug, Default)]
pub struct IconAssets;

impl IconAssets {
    /// Serves the bundled hand-drawn icon set, falling back to `inner` for
    /// every path this crate does not provide itself.
    ///
    /// The editor draws all of its chrome through [`icon`], which resolves to
    /// `icons/<name>.svg`. Those SVGs ship with this crate and are *not* part
    /// of `gpui_kit::assets::Assets`, so an application that only registers the
    /// component bundle renders the toolbar, menus and panels with blank
    /// glyphs. Wrap your application's asset source with this one to fix that:
    ///
    /// ```rust,no_run
    /// use std::borrow::Cow;
    ///
    /// use gpui_excalidraw::ui::icons::IconAssets;
    /// use gpui_kit::*;
    ///
    /// struct AppAssets;
    ///
    /// impl AssetSource for AppAssets {
    ///     fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    ///         IconAssets.load_with(path, |p| gpui_kit::assets::Assets.load(p))
    ///     }
    ///
    ///     fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    ///         IconAssets.list_with(path, |p| gpui_kit::assets::Assets.list(p))
    ///     }
    /// }
    /// ```
    ///
    /// Named `load_with` rather than `load` even though it is the natural
    /// spelling: an inherent `load` of a different arity would shadow
    /// [`AssetSource::load`] on an `IconAssets` value, so `IconAssets.load(path)`
    /// — the call every framework makes through the trait — would stop
    /// compiling. [`IconAssets::list_with`] follows the same convention.
    pub fn load_with<F>(&self, path: &str, inner: F) -> Result<Option<Cow<'static, [u8]>>>
    where
        F: Fn(&str) -> Result<Option<Cow<'static, [u8]>>>,
    {
        let name = path.strip_prefix(PREFIX).unwrap_or(path);
        if name.is_empty() {
            return Ok(None);
        }
        if let Some((_, svg)) = ICON_FILES.iter().find(|(file, _)| *file == name) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }

        inner(path)
    }

    /// Lists the bundled icon set plus whatever `inner` reports for `path`.
    pub fn list_with<F>(&self, path: &str, inner: F) -> Result<Vec<SharedString>>
    where
        F: Fn(&str) -> Result<Vec<SharedString>>,
    {
        let mut names: Vec<SharedString> = ICON_FILES
            .iter()
            .filter_map(|(file, _)| {
                let full = format!("{PREFIX}{file}");
                full.starts_with(path).then(|| full.into())
            })
            .collect();
        names.extend(inner(path).unwrap_or_default());
        Ok(names)
    }
}

impl AssetSource for IconAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        IconAssets::load_with(self, path, |p| gpui_kit::assets::Assets.load(p))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        IconAssets::list_with(self, path, |p| gpui_kit::assets::Assets.list(p))
    }
}

pub fn icon_path(name: &str) -> SharedString {
    format!("{PREFIX}{name}.svg").into()
}

pub fn has_icon(name: &str) -> bool {
    ICON_FILES
        .iter()
        .any(|(file, _)| *file == format!("{name}.svg"))
}

pub fn icon_names() -> impl Iterator<Item = &'static str> {
    ICON_FILES
        .iter()
        .map(|(file, _)| file.strip_suffix(".svg").unwrap_or(file))
}

pub fn icon_source(name: &str) -> Option<&'static str> {
    ICON_FILES
        .iter()
        .find(|(file, _)| *file == format!("{name}.svg"))
        .map(|(_, svg)| *svg)
}

pub fn icon(name: &str, size: f32, color: Rgba) -> impl IntoElement + use<> {
    debug_assert!(
        has_icon(name),
        "{name} is not in the bundled icon set — check the spelling against assets/icons/"
    );
    svg()
        .path(icon_path(name))
        .flex_shrink_0()
        .size(px(size))
        .text_color(color)
}
