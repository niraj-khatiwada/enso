use super::{Choice, catalog, render, valid_emoji};
use crate::Enso;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    assets::IconName,
    component::{
        ActiveTheme, Icon, Sizable,
        button::{Button, ButtonVariants},
        color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
        h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    *,
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

const COLORS: &[(&str, Option<&str>)] = &[
    ("Default", None),
    ("Red", Some("#e25555")),
    ("Orange", Some("#d98236")),
    ("Yellow", Some("#bf9b30")),
    ("Green", Some("#4a9c67")),
    ("Blue", Some("#498add")),
    ("Purple", Some("#9966cc")),
    ("Pink", Some("#d5689c")),
];

// Complete Unicode emoji data, rendered with native system fonts.
struct Emoji {
    value: &'static str,
    label: &'static str,
    keywords: String,
    code: String,
    aliases: Vec<&'static str>,
}

fn emojis() -> &'static [Emoji] {
    static EMOJI: OnceLock<Vec<Emoji>> = OnceLock::new();
    EMOJI.get_or_init(|| {
        include_str!("emoji.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|line| {
                let (value, label) = line.split_once('\t')?;
                let points: Vec<_> = value.chars().collect();
                let code: String = if points.len() == 2
                    && points
                        .iter()
                        .all(|c| (0x1F1E6..=0x1F1FF).contains(&(*c as u32)))
                {
                    points
                        .iter()
                        .filter_map(|c| char::from_u32(*c as u32 - 0x1F1E6 + b'a' as u32))
                        .collect()
                } else {
                    String::new()
                };
                let aliases = match code.as_str() {
                    "us" => vec!["usa", "america", "american", "united states of america"],
                    "gb" => vec!["uk", "britain", "british", "united kingdom"],
                    "ae" => vec!["uae"],
                    "kr" => vec!["south korea"],
                    "kp" => vec!["north korea"],
                    "cz" => vec!["czechia"],
                    "tr" => vec!["turkiye", "türkiye"],
                    "nl" => vec!["holland"],
                    "ci" => vec!["ivory coast"],
                    _ => Vec::new(),
                };
                let extra = if label.contains("technologist") {
                    "developer programmer coding code"
                } else if label == "memo" {
                    "note notes writing"
                } else {
                    ""
                };
                let keywords = format!(
                    "{} {} {} {}",
                    label.to_lowercase().replace(['-', ':'], " "),
                    code,
                    aliases.join(" "),
                    extra
                );
                Some(Emoji {
                    value,
                    label,
                    keywords,
                    code,
                    aliases,
                })
            })
            .collect()
    })
}

pub struct Picker {
    root: PathBuf,
    path: PathBuf,
    owner: WeakEntity<Enso>,
    pub search: Entity<InputState>,
    emoji: bool,
    color: Option<String>,
    icon: String,
    current_emoji: Option<String>,
    emoji_commit: Option<Task<()>>,
    pub(crate) color_picker: Entity<ColorPickerState>,
    _color_change: Subscription,
    _color_open: Subscription,
    _change: Subscription,
}

impl Picker {
    pub fn new(
        root: PathBuf,
        path: PathBuf,
        current: Option<Choice>,
        owner: WeakEntity<Enso>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search Icons…"));
        search.update(cx, |input, cx| input.focus(window, cx));
        let change = cx.subscribe_in(
            &search,
            window,
            |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.emoji_commit = None;
                        let value = input.read(cx).value().to_string();
                        if this.emoji && valid_emoji(&value) {
                            this.emoji_commit = Some(cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(150))
                                    .await;
                                let _ = this.update(cx, |this, cx| {
                                    if this.emoji && this.search.read(cx).value().as_ref() == value
                                    {
                                        this.choose(Choice::Emoji { value }, true, cx);
                                    }
                                });
                            }));
                        }
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => {
                        if let Some((_, choice)) = this.results(cx).into_iter().next() {
                            this.choose(choice, true, cx);
                        }
                    }
                    _ => {}
                }
                let _ = window;
            },
        );
        let (emoji, color, icon, current_emoji) = match current {
            Some(Choice::Icon { name, color }) => (false, color, name, None),
            Some(Choice::Emoji { value }) => (true, None, "file".into(), Some(value)),
            None => (false, None, "file".into(), None),
        };
        if emoji {
            search.update(cx, |s, cx| {
                s.set_placeholder("Search or Paste an Emoji…", window, cx)
            });
        }
        let initial = color
            .as_deref()
            .and_then(super::color)
            .unwrap_or(cx.theme().muted_foreground);
        let color_picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial));
        let popup_owner = owner.clone();
        let color_open = cx.observe(&color_picker, move |_, _, cx| {
            // The nested palette extends beyond the icon chooser's surface.
            // Refresh its outside-click policy when the palette opens/closes.
            let _ = popup_owner.update(cx, |_, cx| cx.notify());
        });
        let color_change = cx.subscribe_in(
            &color_picker,
            window,
            |this, _, event: &ColorPickerEvent, _, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    let hex = crate::rich::highlight_hex(*color);
                    this.color = Some(hex.clone());
                    this.choose(
                        Choice::Icon {
                            name: this.icon.clone(),
                            color: Some(hex),
                        },
                        false,
                        cx,
                    );
                    cx.notify();
                }
            },
        );
        Self {
            root,
            path,
            owner,
            search,
            emoji,
            color,
            icon,
            current_emoji,
            emoji_commit: None,
            color_picker,
            _color_change: color_change,
            _color_open: color_open,
            _change: change,
        }
    }

    fn results(&self, cx: &App) -> Vec<(String, Choice)> {
        let query = self
            .search
            .read(cx)
            .value()
            .trim()
            .to_lowercase()
            .replace('-', " ");
        if self.emoji && valid_emoji(&query) {
            return vec![("Use Emoji".into(), Choice::Emoji { value: query })];
        }
        let matches = |name: &str| query.split_whitespace().all(|word| name.contains(word));
        if self.emoji {
            let mut results: Vec<_> = emojis()
                .iter()
                .filter(|emoji| matches(&emoji.keywords))
                .collect();
            if !query.is_empty() {
                results.sort_by_key(|emoji| {
                    if emoji.code == query || emoji.aliases.iter().any(|alias| *alias == query) {
                        0
                    } else {
                        1
                    }
                });
            }
            results
                .into_iter()
                .map(|emoji| {
                    (
                        emoji.label.to_string(),
                        Choice::Emoji {
                            value: emoji.value.to_string(),
                        },
                    )
                })
                .collect()
        } else {
            catalog()
                .iter()
                .filter(|(name, _)| matches(&name.replace('-', " ")))
                .map(|(name, _)| {
                    (
                        name.replace('-', " "),
                        Choice::Icon {
                            name: name.clone(),
                            color: self.color.clone(),
                        },
                    )
                })
                .collect()
        }
    }

    pub(crate) fn choose(&mut self, choice: Choice, close: bool, cx: &mut Context<Self>) {
        let (root, path) = (self.root.clone(), self.path.clone());
        let _ = self.owner.update(cx, |app, cx| {
            app.apply_file_icon(&root, &path, Some(choice), close, cx)
        });
    }

    fn tab(&mut self, emoji: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.emoji = emoji;
        self.search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(
                if emoji {
                    "Search or Paste an Emoji…"
                } else {
                    "Search Icons…"
                },
                window,
                cx,
            );
            s.focus(window, cx);
        });
        cx.notify();
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut tabs = h_flex().w_full().gap_1();
        for (emoji, label) in [(false, "Icons"), (true, "Emoji")] {
            tabs = tabs.child(
                div()
                    .debug_selector(move || {
                        if emoji {
                            "file-icon-emoji-tab".into()
                        } else {
                            "file-icon-icons-tab".into()
                        }
                    })
                    .child(
                        Button::new(label)
                            .ghost()
                            .xsmall()
                            .label(label)
                            .when(self.emoji == emoji, |b| b.bg(theme.secondary))
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.tab(emoji, window, cx)),
                            ),
                    ),
            );
        }
        tabs = tabs.child(div().flex_1()).child(
            div().debug_selector(|| "file-icon-reset".into()).child(
                Button::new("reset-icon")
                    .ghost()
                    .xsmall()
                    .label("Reset")
                    .tooltip("Restore the Default File Icon")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let (root, path) = (this.root.clone(), this.path.clone());
                        let _ = this.owner.update(cx, |app, cx| {
                            app.apply_file_icon(&root, &path, None, true, cx)
                        });
                    })),
            ),
        );
        let mut content = v_flex()
            .id("file-icon-picker")
            .w(px(288.))
            .gap_2()
            .child(tabs)
            .child(
                Input::new(&self.search)
                    .small()
                    .prefix(Icon::new(IconName::Search).size_3p5()),
            );
        if !self.emoji {
            let mut colors = h_flex().gap_1().w_full().justify_between();
            for (i, (name, color)) in COLORS.iter().enumerate() {
                let color = color.map(str::to_string);
                let tint: Hsla = color
                    .as_ref()
                    .and_then(|c| u32::from_str_radix(&c[1..], 16).ok())
                    .map(rgb)
                    .map(Into::into)
                    .unwrap_or(theme.muted_foreground);
                colors = colors.child(
                    div()
                        .debug_selector(move || format!("file-icon-color-{i}"))
                        .child(
                            Button::new(("icon-color", i))
                                .ghost()
                                .xsmall()
                                .w(px(26.))
                                .h(px(26.))
                                .p_0()
                                .tooltip(*name)
                                .accessibility_label(*name)
                                .when(self.color == color, |b| b.bg(theme.secondary))
                                .child(div().size(px(12.)).rounded_full().bg(tint))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.color = color.clone();
                                    this.color_picker.update(cx, |picker, cx| {
                                        picker.set_value(tint, window, cx)
                                    });
                                    this.choose(
                                        Choice::Icon {
                                            name: this.icon.clone(),
                                            color: color.clone(),
                                        },
                                        false,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        ),
                );
            }
            colors = colors.child(
                div()
                    .debug_selector(|| "file-icon-custom-color".into())
                    .child(
                        ColorPicker::new(&self.color_picker)
                            .icon(IconName::Palette)
                            .xsmall()
                            .accessibility_label("Custom Icon Color"),
                    ),
            );
            content = content.child(colors);
        }
        let results = Arc::new(self.results(cx));
        let rows = results.len().div_ceil(8);
        let weak = cx.entity().downgrade();
        let chosen = if self.emoji {
            self.current_emoji
                .clone()
                .map(|value| Choice::Emoji { value })
        } else {
            Some(Choice::Icon {
                name: self.icon.clone(),
                color: self.color.clone(),
            })
        };
        let foreground = theme.muted_foreground;
        let background = theme.secondary;
        let query = self.search.read(cx).value().to_string();
        if rows == 0 {
            content = content.child(
                div()
                    .h(px(216.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(foreground)
                    .child("No Matches"),
            );
        } else {
            content = content.child(
                uniform_list(
                    SharedString::from(format!("icon-grid-{}-{query}", self.emoji)),
                    rows,
                    move |range, _, _| {
                        range
                            .map(|row| {
                                let mut line = h_flex().gap_1().h(px(36.));
                                for index in row * 8..((row + 1) * 8).min(results.len()) {
                                    let (label, choice) = &results[index];
                                    let choice = choice.clone();
                                    let weak = weak.clone();
                                    line = line.child(
                                        Button::new(("icon-choice", index))
                                            .ghost()
                                            .xsmall()
                                            .w(px(32.))
                                            .h(px(32.))
                                            .p_0()
                                            .tooltip(label.clone())
                                            .accessibility_label(label.clone())
                                            .when(chosen.as_ref() == Some(&choice), |b| {
                                                b.bg(background)
                                            })
                                            .child(render(
                                                Some(&choice),
                                                IconName::File,
                                                foreground,
                                            ))
                                            .on_click(move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.choose(choice.clone(), true, cx)
                                                });
                                            }),
                                    );
                                }
                                line.into_any_element()
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .h(px(216.))
                .w_full(),
            );
        }
        #[cfg(target_os = "macos")]
        if self.emoji {
            content = content.child(
                Button::new("system-emoji-picker")
                    .ghost()
                    .small()
                    .w_full()
                    .label("System Emoji Picker")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.search.update(cx, |s, cx| {
                            s.set_value("", window, cx);
                            s.focus(window, cx);
                        });
                        window.show_character_palette();
                    })),
            );
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use ::core::prelude::v1::test;
    #[test]
    fn unicode_catalog_includes_flags_and_complete_sequences() {
        let catalog = super::emojis();
        assert!(catalog.len() > 3900);
        for value in ["🇺🇸", "🇳🇵", "👩🏽‍💻", "🏳️‍🌈", "🫠"] {
            assert!(
                catalog.iter().any(|emoji| emoji.value == value),
                "missing {value}"
            );
        }
        let usa = catalog.iter().find(|emoji| emoji.value == "🇺🇸").unwrap();
        assert!(usa.keywords.contains("usa") && usa.keywords.contains("united states"));
        for emoji in catalog {
            assert!(
                super::valid_emoji(emoji.value),
                "invalid sequence {}: {}",
                emoji.label,
                emoji.value
            );
        }
    }
}
