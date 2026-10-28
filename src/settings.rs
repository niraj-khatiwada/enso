use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Sizable, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants},
    h_flex, v_flex,
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
    window: Option<AnyWindowHandle>,
    path: Option<PathBuf>,
}
impl Global for Preferences {}

actions!(settings, [CloseSettings]);

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("Enso").join("appearance"))
}
fn load(path: &Path) -> Appearance {
    std::fs::read_to_string(path)
        .map(|value| Appearance::parse(&value))
        .unwrap_or_default()
}

pub fn init(cx: &mut App) {
    let path = path();
    let appearance = path.as_ref().map(|p| load(p)).unwrap_or_default();
    cx.set_global(Preferences {
        appearance,
        window: None,
        path,
    });
    apply(appearance, cx);
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
fn apply(appearance: Appearance, cx: &mut App) {
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
                    title: Some("Enso Settings".into()),
                    ..Default::default()
                }
            }),
            app_owns_titlebar_drag: cfg!(target_os = "macos"),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(640.), px(440.)),
                cx,
            ))),
            window_min_size: Some(size(px(580.), px(400.))),
            ..Default::default()
        },
        cx,
        |window, cx| {
            cx.new(|cx| {
                window.set_window_title("Enso Settings");
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                SettingsWindow {
                    focus,
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

struct SettingsWindow {
    focus: FocusHandle,
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

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected = current(cx);
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
                v_flex()
                    .flex_1()
                    .p_8()
                    .gap_6()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Appearance"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Choose how Enso looks on your device."),
                            ),
                    )
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
                                                                .child(
                                                                    div().text_sm().child(label),
                                                                ),
                                                        ),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.error =
                                                        choose(appearance, cx).err().map(|error| {
                                                            format!(
                                                                "Could not save appearance: {error}"
                                                            )
                                                        });
                                                    cx.notify();
                                                }))
                                        },
                                    ),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if self.error.is_some() {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .child(self.error.clone().unwrap_or_else(|| {
                                if selected == Appearance::System {
                                    "Follows your system appearance automatically.".into()
                                } else {
                                    "Applies to all Enso windows. Saved automatically.".into()
                                }
                            })),
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
