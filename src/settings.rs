use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Theme, ThemeMode, ThemeRegistry, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    switch::Switch,
    v_flex,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}
impl Appearance {
    fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
    fn parse(value: &str) -> Self {
        match value.trim() {
            "dark" => Self::Dark,
            "light" => Self::Light,
            _ => Self::System,
        }
    }
}

pub struct Preferences {
    pub appearance: Appearance,
    pub reading: crate::typography::ReadingStyle,
    pub(crate) reading_path: Option<PathBuf>,
    /// Show line numbers beside the editor's text.
    pub line_numbers: bool,
    pub document_outline: bool,
    pub spell_check: bool,
    /// Load images from the web (`https://…`) in notes.
    pub web_images: bool,
    /// Ask before a file or folder goes to the Trash.
    pub confirm_delete: bool,
    /// Split view puts the reader on the left and the editor on the right.
    pub reader_first: bool,
    pub(crate) split_order_path: Option<PathBuf>,
    /// The sidebar is open; every window and workspace opens it this way.
    pub sidebar_open: bool,
    window: Option<AnyWindowHandle>,
    path: Option<PathBuf>,
}
impl Global for Preferences {}

actions!(settings, [CloseSettings]);

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("appearance"))
}
fn line_numbers_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("line-numbers"))
}
fn document_outline_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("document-outline"))
}
fn load_document_outline(path: &Path) -> bool {
    std::fs::read_to_string(path).map_or(true, |value| value.trim() != "off")
}
fn web_images_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("web-images"))
}
pub fn web_images(cx: &App) -> bool {
    cx.try_global::<Preferences>().is_none_or(|p| p.web_images)
}
/// Whether notes load images from the web. Off, no image request leaves the
/// app; images already shown stay until the note is reopened.
fn set_web_images(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() { init(cx); }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.web_images = on);
    crate::http::set_enabled(on);
    if let Some(path) = web_images_path() {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, if on { "on" } else { "off" })?;
    }
    Ok(())
}
fn confirm_delete_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("confirm-delete"))
}
pub fn confirm_delete(cx: &App) -> bool {
    cx.try_global::<Preferences>().is_none_or(|p| p.confirm_delete)
}
fn set_confirm_delete(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() { init(cx); }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.confirm_delete = on);
    if let Some(path) = confirm_delete_path() {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, if on { "on" } else { "off" })?;
    }
    Ok(())
}
fn spell_check_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("spell-check"))
}
pub fn spell_check(cx: &App) -> bool {
    cx.try_global::<Preferences>().is_none_or(|p| p.spell_check)
}
fn set_spell_check(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() { init(cx); }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.spell_check = on);
    if let Some(path) = spell_check_path() {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, if on { "on" } else { "off" })?;
    }
    Ok(())
}
pub fn document_outline(cx: &App) -> bool {
    cx.try_global::<Preferences>().is_none_or(|p| p.document_outline)
}
fn set_document_outline(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() { init(cx); }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.document_outline = on);
    if let Some(path) = document_outline_path() {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, if on { "on" } else { "off" })?;
    }
    Ok(())
}
fn sidebar_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("sidebar"))
}
fn load_sidebar_open(path: &Path) -> bool {
    std::fs::read_to_string(path).map_or(true, |value| value.trim() != "closed")
}

/// Whether windows open with the sidebar shown: open unless last collapsed.
pub fn sidebar_open(cx: &App) -> bool {
    cx.try_global::<Preferences>().is_none_or(|p| p.sidebar_open)
}

/// Remember the sidebar as open or collapsed, for every window and session.
pub fn set_sidebar_open(open: bool, cx: &mut App) -> io::Result<()> {
    // Tests toggle the sidebar too, and mustn't change the real preference.
    if cfg!(test) {
        return Ok(());
    }
    if !cx.has_global::<Preferences>() {
        init(cx);
    }
    if cx.global::<Preferences>().sidebar_open == open {
        return Ok(());
    }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.sidebar_open = open);
    if let Some(path) = sidebar_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, if open { "open" } else { "closed" })?;
    }
    Ok(())
}

fn split_order_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("split-order"))
}
fn load_reader_first(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|value| value.trim() == "reader-first")
}

/// Whether split view shows the reader on the left; the editor leads unless
/// the user swapped them.
pub fn reader_first(cx: &App) -> bool {
    cx.try_global::<Preferences>()
        .is_some_and(|p| p.reader_first)
}

/// Swap the split view's sides in every window, and remember it.
pub fn set_reader_first(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() {
        init(cx);
    }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.reader_first = on);
    if let Some(path) = cx.global::<Preferences>().split_order_path.clone() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, if on { "reader-first" } else { "editor-first" })?;
    }
    Ok(())
}

fn load_line_numbers(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|value| value.trim() == "on")
}

/// Whether editors show line numbers; off unless turned on in Settings.
pub fn line_numbers(cx: &App) -> bool {
    cx.try_global::<Preferences>()
        .is_some_and(|p| p.line_numbers)
}

fn set_line_numbers(on: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() {
        init(cx);
    }
    // Observers (every open workspace window) restyle their editors from this.
    cx.update_global::<Preferences, _>(|prefs, _| prefs.line_numbers = on);
    if let Some(path) = line_numbers_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, if on { "on" } else { "off" })?;
    }
    Ok(())
}

fn load(path: &Path) -> Appearance {
    std::fs::read_to_string(path)
        .map(|value| Appearance::parse(&value))
        .unwrap_or_default()
}

pub fn init(cx: &mut App) {
    let path = path();
    let appearance = path.as_ref().map(|p| load(p)).unwrap_or_default();
    let line_numbers = line_numbers_path().as_deref().is_some_and(load_line_numbers);
    let document_outline = document_outline_path().as_deref().is_none_or(load_document_outline);
    let spell_check = spell_check_path().as_deref().is_none_or(load_document_outline);
    let web_images = web_images_path().as_deref().is_none_or(load_document_outline);
    crate::http::set_enabled(web_images);
    let confirm_delete = confirm_delete_path().as_deref().is_none_or(load_document_outline);
    let split_order_path = split_order_path();
    let reader_first = split_order_path.as_deref().is_some_and(load_reader_first);
    let sidebar_open = sidebar_path().as_deref().is_none_or(load_sidebar_open);
    let reading_path = crate::typography::preference_path();
    let reading = reading_path.as_deref()
        .map(crate::typography::ReadingStyle::load).unwrap_or_default();
    cx.set_global(Preferences {
        appearance,
        reading,
        reading_path,
        line_numbers,
        document_outline,
        spell_check,
        web_images,
        confirm_delete,
        reader_first,
        split_order_path,
        sidebar_open,
        window: None,
        path,
    });
    apply(appearance, cx);
    // The kit reloads the active theme whenever its registry changes (bundled
    // themes finishing loading, a theme file edited on disk), which replaces
    // the gutter color set above. Registered after the kit's own observer, so
    // this runs once its reload is done.
    cx.observe_global::<ThemeRegistry>(tune_theme).detach();
    cx.bind_keys([
        KeyBinding::new("cmd-w", CloseSettings, Some("EnsoSettings")),
        KeyBinding::new("ctrl-w", CloseSettings, Some("EnsoSettings")),
    ]);
}

fn current(cx: &App) -> Appearance {
    cx.try_global::<Preferences>()
        .map(|p| p.appearance)
        .unwrap_or_default()
}
/// Give the editor's line-number gutter exactly the color its text area shows.
/// (Over a translucent window that means none at all: see below.)
/// The editor background is translucent in dark mode, and the gutter paints it
/// a second time over the text area's, which reads as a lighter panel; so the
/// gutter gets the already-composited, opaque color instead.
fn blend_gutter(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    let editor = theme
        .highlight_theme
        .style
        .editor_background
        .unwrap_or_else(|| theme.input_background());
    // Over a translucent window the text area's color depends on the desktop
    // behind it, so no fixed color can match it: leave the gutter clear.
    let gutter = if cfg!(any(target_os = "macos", target_os = "windows")) {
        gpui_kit::transparent_black()
    } else {
        theme.background.blend(editor)
    };
    if theme.highlight_theme.style.editor_gutter_background != Some(gutter) {
        let mut highlight = (*theme.highlight_theme).clone();
        highlight.style.editor_gutter_background = Some(gutter);
        theme.highlight_theme = std::sync::Arc::new(highlight);
        cx.refresh_windows();
    }
}

/// Enso's adjustments on top of GPUI Kit's theme. The kit rebuilds its theme
/// on every change, so this runs again after each one.
fn tune_theme(cx: &mut App) {
    blend_gutter(cx);
    tone_surfaces(cx);
    thin_scrollbars(cx);
}

/// Floating surfaces stay opaque so document text never shows through them.
/// Use softer neutral tones instead of pure black and white. In dark mode the
/// surface stays close to the window's own neutral dark; its border and
/// shadow (`ui::SurfaceExt`) lift it, not a lighter, bluer fill.
fn tone_surfaces(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    let surface: Hsla = if theme.mode.is_dark() { rgb(0x1f1f1f).into() } else { rgb(0xf5f5f3).into() };
    theme.popover = surface;
    theme.tokens.popover = surface.into();
    // The highlighted row in menus and the command palette. The kit's own
    // accent is a near-white in light mode, invisible on the surface above,
    // so it's set a clear step away from that surface instead.
    let accent: Hsla = if theme.mode.is_dark() { rgb(0x2e2e2e).into() } else { rgb(0xe6e6e3).into() };
    theme.accent = accent;
    theme.tokens.accent = accent.into();
}

/// Hairline scrollbar thumbs, a little wider while hovered or dragged. The kit's
/// own are 6 and 8 px.
fn thin_scrollbars(cx: &mut App) {
    let base = gpui_kit::base::Theme::global_mut(cx);
    let styles = base
        .scrollbar
        .styles()
        .clone()
        .thumb(|thumb| thumb.width(px(3.)).inset(px(2.)))
        .thumb_hover(|thumb| thumb.width(px(4.)).inset(px(2.)))
        .thumb_active(|thumb| thumb.width(px(4.)).inset(px(2.)));
    base.scrollbar = base.scrollbar.clone().with_styles(styles);
    cx.refresh_windows();
}

fn apply(appearance: Appearance, cx: &mut App) {
    apply_theme(appearance, cx);
    tune_theme(cx);
}

fn apply_theme(appearance: Appearance, cx: &mut App) {
    match appearance {
        Appearance::System => {
            cx.set_window_appearance(None);
            Theme::sync_system_appearance(None, cx);
        }
        Appearance::Dark => {
            cx.set_window_appearance(Some(WindowAppearance::Dark));
            Theme::change(ThemeMode::Dark, None, cx);
        }
        Appearance::Light => {
            cx.set_window_appearance(Some(WindowAppearance::Light));
            Theme::change(ThemeMode::Light, None, cx);
        }
    }
}

pub fn watch_system(window: &Window) -> Subscription {
    window.observe_window_appearance(|window, cx| {
        if current(cx) == Appearance::System {
            let mode = ThemeMode::from(window.appearance());
            if cx.theme().mode != mode {
                Theme::change(mode, None, cx);
                tune_theme(cx);
            }
        }
    })
}

fn choose(appearance: Appearance, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<Preferences>() {
        init(cx);
    }
    cx.update_global::<Preferences, _>(|prefs, _| prefs.appearance = appearance);
    apply(appearance, cx);
    if let Some(path) = cx.global::<Preferences>().path.clone() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, appearance.name())?;
    }
    Ok(())
}

pub fn open(cx: &mut App) {
    if !cx.has_global::<Preferences>() {
        init(cx);
    }
    if let Some(handle) = cx.global::<Preferences>().window {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }
    let result = gpui_kit::open_window(
        WindowOptions {
            titlebar: Some(if cfg!(target_os = "macos") {
                TitlebarOptions {
                    traffic_light_position: Some(point(px(12.), px(14.))),
                    ..TitleBar::title_bar_options()
                }
            } else {
                TitlebarOptions {
                    title: Some("enso Settings".into()),
                    ..Default::default()
                }
            }),
            app_owns_titlebar_drag: cfg!(target_os = "macos"),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(760.), px(480.)),
                cx,
            ))),
            window_min_size: Some(size(px(680.), px(420.))),
            ..Default::default()
        },
        cx,
        |window, cx| {
            cx.new(|cx| {
                window.set_window_title("enso Settings");
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                SettingsWindow {
                    focus,
                    category: Category::Appearance,
                    error: None,
                    _appearance: watch_system(window),
                    _preferences: cx.observe_global::<Preferences>(|_, cx| cx.notify()),
                }
            })
        },
    );
    if let Ok((handle, _)) = result {
        cx.update_global::<Preferences, _>(|prefs, _| prefs.window = Some(handle));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Appearance,
    Editor,
}

impl Category {
    const ALL: [Category; 2] = [Category::Appearance, Category::Editor];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Palette,
            Self::Editor => IconName::Pencil,
        }
    }
}

struct SettingsWindow {
    focus: FocusHandle,
    category: Category,
    error: Option<String>,
    _appearance: Subscription,
    _preferences: Subscription,
}

fn preview(appearance: Appearance) -> impl IntoElement {
    let halves: &[bool] = match appearance {
        Appearance::System => &[false, true],
        Appearance::Light => &[false],
        Appearance::Dark => &[true],
    };
    h_flex()
        .w_full()
        .h(px(66.))
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(rgb(0x888888).opacity(0.2))
        .children(halves.iter().map(|dark| {
            let background = if *dark { rgb(0x202124) } else { rgb(0xffffff) };
            let sidebar = if *dark { rgb(0x2e3034) } else { rgb(0xf0f1f3) };
            let ink = if *dark { rgb(0x60636a) } else { rgb(0xd4d6db) };
            h_flex()
                .flex_1()
                .h_full()
                .bg(background)
                .child(div().w(px(18.)).h_full().bg(sidebar))
                .child(
                    v_flex()
                        .flex_1()
                        .p_2()
                        .gap_2()
                        .child(div().w(relative(0.7)).h(px(4.)).rounded_sm().bg(ink))
                        .child(div().w_full().h(px(3.)).rounded_sm().bg(ink.opacity(0.5)))
                        .child(
                            div()
                                .w(relative(0.8))
                                .h(px(3.))
                                .rounded_sm()
                                .bg(ink.opacity(0.5)),
                        ),
                )
        }))
}

fn page_header(title: &'static str, description: &'static str, muted: Hsla) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(div().text_sm().text_color(muted).child(description))
}

impl SettingsWindow {
    fn appearance_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected = current(cx);
        v_flex()
            .gap_6()
            .child(page_header(
                "Appearance",
                "Choose how enso looks on your device.",
                theme.muted_foreground,
            ))
            .child(
                v_flex()
                    .gap_3()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Theme"))
                    .child(
                        h_flex().gap_3().children(
                            [
                                (Appearance::System, "System", IconName::Monitor),
                                (Appearance::Dark, "Dark", IconName::Moon),
                                (Appearance::Light, "Light", IconName::Sun),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (appearance, label, icon))| {
                                Button::new(("appearance", index))
                                    .ghost()
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(132.))
                                    .p_3()
                                    .border_1()
                                    .border_color(if selected == appearance {
                                        theme.primary
                                    } else {
                                        theme.border
                                    })
                                    .when(selected == appearance, |button| {
                                        button.bg(theme.secondary)
                                    })
                                    .accessibility_label(format!("{label} theme"))
                                    .child(
                                        v_flex()
                                            .w_full()
                                            .gap_3()
                                            .child(preview(appearance))
                                            .child(
                                                h_flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .gap_2()
                                                    .child(icon)
                                                    .child(div().text_sm().child(label)),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.error = choose(appearance, cx).err().map(|error| {
                                            format!("Could not save appearance: {error}")
                                        });
                                        cx.notify();
                                    }))
                            }),
                        ),
                    ),
            )
            .child({
                use crate::typography::{ReadingFont, current, set};
                let reading = current(cx);
                v_flex().gap_3()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Font Style"))
                    .child(v_flex().gap_2().children(ReadingFont::ALL.chunks(3).enumerate().map(|(row, fonts)| {
                        h_flex().gap_2().children(fonts.iter().copied().enumerate().map(|(col, font)| {
                            let selected = font == reading.font;
                            Button::new(("reading-font-card", row * 3 + col)).ghost()
                                .flex_1().min_w_0().h(px(82.)).p_2()
                                .border_1().border_color(if selected { theme.primary } else { theme.border })
                                .when(selected, |b| b.bg(theme.secondary))
                                .accessibility_label(format!("{} reading font", font.name()))
                                .child(v_flex().gap_1().items_start().w_full()
                                    .child(div().font_family(font.family(cx)).text_size(px(24.)).child("Aa"))
                                    .child(div().text_xs().child(font.name())))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.error = set(crate::typography::ReadingStyle { font, ..current(cx) }, true, cx)
                                        .err().map(|e| format!("Could not save reading font: {e}"));
                                    cx.notify();
                                }))
                        }))
                    })))
            })
    }

    fn editor_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let line_numbers = line_numbers(cx);
        let outline = document_outline(cx);
        let outline_weak = cx.entity().downgrade();
        let weak = cx.entity().downgrade();
        v_flex()
            .gap_6()
            .child(page_header(
                "Editor",
                "How the writing pane looks while you edit.",
                theme.muted_foreground,
            ))
            .child({
                let weak = cx.entity().downgrade();
                h_flex().gap_4().items_center().justify_between().py_3()
                    .border_b_1().border_color(theme.border)
                    .child(v_flex().gap_0p5()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Spell Check"))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            if cfg!(target_os = "linux") { "Underline misspellings using Enchant and your system dictionaries." }
                            else { "Underline misspellings using your system’s spelling service." })))
                    .child(Switch::new("spell-check").checked(spell_check(cx))
                        .accessibility_label("Spell Check")
                        .on_change(move |on, _, cx| {
                            let error = set_spell_check(*on, cx).err().map(|e| format!("Could not save spell check: {e}"));
                            let _ = weak.update(cx, |this, cx| { this.error = error; cx.notify(); });
                        }))
            })
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .justify_between()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Line numbers"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Show line numbers beside the text in Edit and Split views."),
                            ),
                    )
                    .child(
                        Switch::new("line-numbers")
                            .checked(line_numbers)
                            .accessibility_label("Line numbers")
                            .on_change(move |on, _, cx| {
                                let error = set_line_numbers(*on, cx)
                                    .err()
                                    .map(|error| format!("Could not save editor settings: {error}"));
                                let _ = weak.update(cx, |this, cx| {
                                    this.error = error;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                h_flex().gap_4().items_center().justify_between().py_3()
                    .border_b_1().border_color(theme.border)
                    .child(v_flex().gap_0p5()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Document Outline"))
                        .child(div().text_xs().text_color(theme.muted_foreground).child("Show the section navigation rail beside Markdown documents.")))
                    .child(Switch::new("document-outline").checked(outline).accessibility_label("Document Outline")
                        .on_change(move |on,_,cx| {
                            let error=set_document_outline(*on,cx).err().map(|e|format!("Could not save editor settings: {e}"));
                            let _=outline_weak.update(cx,|this,cx|{this.error=error;cx.notify();});
                        }))
            )
            .child({
                let weak = cx.entity().downgrade();
                h_flex().gap_4().items_center().justify_between().py_3()
                    .border_b_1().border_color(theme.border)
                    .child(v_flex().gap_0p5()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Load Web Images"))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            "Show images hosted on the web. Off, notes never contact the sites their images come from.")))
                    .child(Switch::new("web-images").checked(web_images(cx))
                        .accessibility_label("Load Web Images")
                        .on_change(move |on, _, cx| {
                            let error = set_web_images(*on, cx).err().map(|e| format!("Could not save editor settings: {e}"));
                            let _ = weak.update(cx, |this, cx| { this.error = error; cx.notify(); });
                        }))
            })
            .child({
                let weak = cx.entity().downgrade();
                h_flex().gap_4().items_center().justify_between().py_3()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Confirm Before Deleting Files & Folders"))
                    .child(Switch::new("confirm-delete").checked(confirm_delete(cx))
                        .accessibility_label("Confirm Before Deleting Files & Folders")
                        .on_change(move |on, _, cx| {
                            let error = set_confirm_delete(*on, cx).err().map(|e| format!("Could not save editor settings: {e}"));
                            let _ = weak.update(cx, |this, cx| { this.error = error; cx.notify(); });
                        }))
            })
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let category = self.category;
        let page = match category {
            Category::Appearance => self.appearance_page(cx).into_any_element(),
            Category::Editor => self.editor_page(cx).into_any_element(),
        };
        v_flex()
            .id("settings-window")
            .key_context("EnsoSettings")
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(|_: &CloseSettings, window, _| window.remove_window())
            .when(cfg!(target_os = "macos"), |view| {
                view.child(
                    TitleBar::new().h(px(44.)).bg(theme.background).child(
                        h_flex()
                            .h_full()
                            .items_center()
                            .child(div().text_sm().child("Settings")),
                    ),
                )
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        v_flex()
                            .w(px(180.))
                            .h_full()
                            .flex_shrink_0()
                            .p_3()
                            .gap_0p5()
                            .bg(theme.sidebar)
                            .border_r_1()
                            .border_color(theme.border)
                            .children(Category::ALL.into_iter().enumerate().map(|(i, item)| {
                                let active = item == category;
                                h_flex()
                                    .id(("settings-category", i))
                                    .h(px(30.))
                                    .px_2()
                                    .gap_2()
                                    .items_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_sm()
                                    .when(active, |row| {
                                        row.bg(theme.secondary.opacity(0.5))
                                            .font_weight(FontWeight::MEDIUM)
                                    })
                                    .when(!active, |row| {
                                        row.text_color(theme.muted_foreground)
                                            .hover(|row| row.bg(theme.secondary.opacity(0.35)))
                                    })
                                    .child(item.icon())
                                    .child(item.label())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.category = item;
                                        this.error = None;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(
                        v_flex()
                            .id("settings-page")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .p_8()
                            .gap_6()
                            .child(div().flex_shrink_0().child(page))
                            .when_some(self.error.clone(), |page, error| {
                                page.child(div().text_xs().text_color(theme.danger).child(error))
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[::core::prelude::v1::test]
    fn outline_defaults_on_and_remembers_saved_choice() {
        let path=std::env::temp_dir().join(format!("enso-outline-setting-{}",std::process::id()));
        let _=std::fs::remove_file(&path);
        assert!(super::load_document_outline(&path));
        std::fs::write(&path,"off").unwrap();
        assert!(!super::load_document_outline(&path));
        std::fs::write(&path,"on").unwrap();
        assert!(super::load_document_outline(&path));
        std::fs::remove_file(path).unwrap();
    }
    #[::core::prelude::v1::test]
    fn appearance_values_round_trip() {
        use super::Appearance;
        for appearance in [Appearance::System, Appearance::Dark, Appearance::Light] {
            assert_eq!(Appearance::parse(appearance.name()), appearance);
        }
        assert_eq!(Appearance::parse("unknown"), Appearance::System);
    }
}
