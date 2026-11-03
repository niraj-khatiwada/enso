use crate::visual::prefs::{BLINK_CELLS, CaretColor, FocusMode, WritingPrefs};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Sizable, Theme, ThemeMode, ThemeRegistry, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    switch::Switch,
    tooltip::Tooltip,
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
    /// Show line numbers beside the editor's text.
    pub line_numbers: bool,
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

fn writing_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| crate::files::app_folder(p).join("writing"))
}

pub fn set_writing(cx: &mut App, change: impl FnOnce(&mut WritingPrefs)) -> io::Result<()> {
    let mut prefs = WritingPrefs::get(cx);
    change(&mut prefs);
    let text = prefs.serialize();
    cx.set_global(prefs);
    if let Some(path) = writing_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
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
    let line_numbers = line_numbers_path()
        .as_deref()
        .is_some_and(load_line_numbers);
    let writing = writing_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| WritingPrefs::parse(&text))
        .unwrap_or_default();
    cx.set_global(writing);
    cx.set_global(Preferences {
        appearance,
        line_numbers,
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
    thin_scrollbars(cx);
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
    Writing,
    Caret,
    Editor,
}

impl Category {
    const ALL: [Category; 4] = [
        Category::Appearance,
        Category::Writing,
        Category::Caret,
        Category::Editor,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Writing => "Writing",
            Self::Caret => "Caret",
            Self::Editor => "Source editor",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Palette,
            Self::Writing => IconName::BookOpen,
            Self::Caret => IconName::TextCursor,
            Self::Editor => IconName::SquareTerminal,
        }
    }
}

const CARET_SWATCHES: [(u32, &str); 5] = [
    (0x2f6fde, "Blue"),
    (0x8a4fff, "Violet"),
    (0xe0457b, "Pink"),
    (0x1f9d55, "Green"),
    (0xe8890c, "Orange"),
];

fn setting_row(
    title: &'static str,
    description: &'static str,
    control: impl IntoElement,
    theme: &Theme,
) -> impl IntoElement {
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
                .min_w_0()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description),
                ),
        )
        .child(control)
}

fn segmented<T: Copy + PartialEq + 'static>(
    id: &'static str,
    choices: Vec<(T, SharedString)>,
    selected: T,
    on_pick: impl Fn(T, &mut App) + Clone + 'static,
    theme: &Theme,
) -> impl IntoElement {
    h_flex()
        .p_0p5()
        .gap_0p5()
        .rounded_md()
        .bg(theme.secondary.opacity(0.5))
        .children(choices.into_iter().enumerate().map(|(i, (value, label))| {
            let on_pick = on_pick.clone();
            let active = value == selected;
            Button::new((id, i))
                .ghost()
                .small()
                .label(label.clone())
                .accessibility_label(label)
                .when(active, |b| b.bg(theme.background))
                .on_click(move |_, _, cx| on_pick(value, cx))
        }))
}

struct SettingsWindow {
    focus: FocusHandle,
    category: Category,
    error: Option<String>,
    _appearance: Subscription,
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
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
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
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Theme"),
                    )
                    .child(
                        h_flex().gap_3().children(
                            [
                                (Appearance::System, "System", IconName::Monitor),
                                (Appearance::Dark, "Dark", IconName::Moon),
                                (Appearance::Light, "Light", IconName::Sun),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(
                                |(index, (appearance, label, icon))| {
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
                                            this.error =
                                                choose(appearance, cx).err().map(|error| {
                                                    format!("Could not save appearance: {error}")
                                                });
                                            cx.notify();
                                        }))
                                },
                            ),
                        ),
                    ),
            )
    }

    fn writing_setter<T: 'static>(
        &self,
        cx: &mut Context<Self>,
        change: fn(&mut WritingPrefs, T),
    ) -> impl Fn(T, &mut App) + Clone + 'static {
        let weak = cx.entity().downgrade();
        move |value, cx| {
            let error = set_writing(cx, |prefs| change(prefs, value))
                .err()
                .map(|error| format!("Could not save writing settings: {error}"));
            let _ = weak.update(cx, |this, cx| {
                this.error = error;
                cx.notify();
            });
        }
    }

    fn writing_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let prefs = WritingPrefs::get(cx);
        let typewriter = self.writing_setter(cx, |p, on: bool| p.typewriter = on);
        let focus = self.writing_setter(cx, |p, mode: FocusMode| p.focus = mode);
        v_flex()
            .gap_6()
            .child(page_header(
                "Writing",
                "How the page behaves while you write.",
                theme.muted_foreground,
            ))
            .child(
                v_flex()
                    .child(setting_row(
                        "Typewriter mode",
                        "Keep the line you are writing at the same height. Drag the guide in the page to move it.",
                        Switch::new("typewriter")
                            .checked(prefs.typewriter)
                            .accessibility_label("Typewriter mode")
                            .on_change(move |on, _, cx| typewriter(*on, cx)),
                        &theme,
                    ))
                    .child(setting_row(
                        "Focus",
                        "Dim everything but the sentence or paragraph you are writing.",
                        segmented(
                            "focus",
                            FocusMode::ALL
                                .into_iter()
                                .map(|m| (m, m.label().into()))
                                .collect(),
                            prefs.focus,
                            focus,
                            &theme,
                        ),
                        &theme,
                    )),
            )
    }

    fn caret_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let prefs = WritingPrefs::get(cx);
        let color = self.writing_setter(cx, |p, c: CaretColor| p.caret_color = c);
        let width = self.writing_setter(cx, |p, w: u32| p.caret_width = w as f32);
        let blink = self.writing_setter(cx, |p, on: bool| p.caret_blink = on);
        let rate = self.writing_setter(cx, |p, ms: u32| p.blink_period_ms = ms);
        let cell = self.writing_setter(cx, |p, i: u32| {
            let flipped = p.blink_pattern ^ (1 << i);
            if flipped != 0 {
                p.blink_pattern = flipped;
            }
        });
        let swatch = |id: usize, value: CaretColor, fill: Hsla, label: &'static str| {
            let color = color.clone();
            let active = prefs.caret_color == value;
            div()
                .id(("caret-color", id))
                .size(px(24.))
                .rounded_full()
                .bg(fill)
                .border_2()
                .border_color(if active {
                    theme.foreground
                } else {
                    theme.border
                })
                .cursor_pointer()
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                .on_click(move |_, _, cx| color(value, cx))
        };
        let mut swatches = vec![
            swatch(0, CaretColor::Theme, theme.caret, "Theme"),
            swatch(1, CaretColor::Accent, theme.primary, "Accent"),
        ];
        for (i, (value, label)) in CARET_SWATCHES.into_iter().enumerate() {
            swatches.push(swatch(
                2 + i,
                CaretColor::Custom(value),
                rgb(value).into(),
                label,
            ));
        }
        let pattern = h_flex().gap_0p5().children((0..BLINK_CELLS).map(|i| {
            let cell = cell.clone();
            let on = prefs.blink_pattern & (1 << i) != 0;
            div()
                .id(("blink-cell", i as usize))
                .w(px(14.))
                .h(px(20.))
                .rounded_sm()
                .border_1()
                .border_color(theme.border)
                .when(on, |c| c.bg(theme.foreground.opacity(0.75)))
                .when(!prefs.caret_blink, |c| c.opacity(0.4))
                .cursor_pointer()
                .on_click(move |_, _, cx| cell(i, cx))
        }));
        v_flex()
            .gap_6()
            .child(page_header(
                "Caret",
                "The caret in the page. The source editor keeps its own.",
                theme.muted_foreground,
            ))
            .child(
                v_flex()
                    .child(setting_row(
                        "Color",
                        "The theme's caret, its accent, or a color of your own.",
                        h_flex().gap_2().children(swatches),
                        &theme,
                    ))
                    .child(setting_row(
                        "Thickness",
                        "Caret width in pixels.",
                        segmented(
                            "caret-width",
                            (1..=4u32).map(|w| (w, format!("{w} px").into())).collect(),
                            prefs.caret_width.round() as u32,
                            width,
                            &theme,
                        ),
                        &theme,
                    ))
                    .child(setting_row(
                        "Blink",
                        "A steady caret, or one that blinks.",
                        Switch::new("caret-blink")
                            .checked(prefs.caret_blink)
                            .accessibility_label("Caret blink")
                            .on_change(move |on, _, cx| blink(*on, cx)),
                        &theme,
                    ))
                    .child(setting_row(
                        "Blink rate",
                        "How long one blink takes.",
                        segmented(
                            "blink-rate",
                            vec![
                                (1400u32, "Slow".into()),
                                (1060, "Medium".into()),
                                (700, "Fast".into()),
                            ],
                            prefs.blink_period_ms,
                            rate,
                            &theme,
                        ),
                        &theme,
                    ))
                    .child(setting_row(
                        "Blink pattern",
                        "One blink in twelve steps: filled steps show the caret.",
                        pattern,
                        &theme,
                    )),
            )
    }

    fn editor_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let line_numbers = line_numbers(cx);
        let weak = cx.entity().downgrade();
        v_flex()
            .gap_6()
            .child(page_header(
                "Editor",
                "How the writing pane looks while you edit.",
                theme.muted_foreground,
            ))
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
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Line numbers"),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                "Show line numbers beside the text in Edit and Split views.",
                            )),
                    )
                    .child(
                        Switch::new("line-numbers")
                            .checked(line_numbers)
                            .accessibility_label("Line numbers")
                            .on_change(move |on, _, cx| {
                                let error = set_line_numbers(*on, cx).err().map(|error| {
                                    format!("Could not save editor settings: {error}")
                                });
                                let _ = weak.update(cx, |this, cx| {
                                    this.error = error;
                                    cx.notify();
                                });
                            }),
                    ),
            )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let category = self.category;
        let page = match category {
            Category::Appearance => self.appearance_page(cx).into_any_element(),
            Category::Writing => self.writing_page(cx).into_any_element(),
            Category::Caret => self.caret_page(cx).into_any_element(),
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
                            .child(page)
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
    fn appearance_values_round_trip() {
        use super::Appearance;
        for appearance in [Appearance::System, Appearance::Dark, Appearance::Light] {
            assert_eq!(Appearance::parse(appearance.name()), appearance);
        }
        assert_eq!(Appearance::parse("unknown"), Appearance::System);
    }
}
