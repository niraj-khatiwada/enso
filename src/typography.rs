//! Reading preferences are shared across documents; app and editor typography
//! remain independent. Fonts are embedded so a plain binary works offline.
use gpui_kit::component::{
    ActiveTheme, IndexPath, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    text::TextView,
    v_flex,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{borrow::Cow, io, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingFont {
    #[default]
    Default,
    Inter,
    Lora,
    Mono,
    Poppins,
    FiraCode,
}

impl ReadingFont {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::Inter,
        Self::Lora,
        Self::Mono,
        Self::Poppins,
        Self::FiraCode,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Inter => "Inter",
            Self::Lora => "Lora",
            Self::Mono => "JetBrains Mono",
            Self::Poppins => "Poppins",
            Self::FiraCode => "Fira Code",
        }
    }
    pub fn family(self, cx: &App) -> SharedString {
        match self {
            Self::Default => cx.theme().font_family.clone(),
            _ => self.name().into(),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Inter => "inter",
            Self::Lora => "lora",
            Self::Mono => "mono",
            Self::Poppins => "poppins",
            Self::FiraCode => "fira-code",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadingStyle {
    pub font: ReadingFont,
    pub size: u8,
}
impl Default for ReadingStyle {
    fn default() -> Self {
        Self {
            font: ReadingFont::Default,
            size: 16,
        }
    }
}
impl ReadingStyle {
    pub const MIN: u8 = 12;
    pub const MAX: u8 = 28;
    fn decode(text: &str) -> Self {
        let mut lines = text.lines();
        let font = match lines.next().unwrap_or("") {
            "inter" => ReadingFont::Inter,
            "lora" => ReadingFont::Lora,
            "mono" => ReadingFont::Mono,
            "poppins" => ReadingFont::Poppins,
            "fira-code" => ReadingFont::FiraCode,
            _ => ReadingFont::Default,
        };
        let size = lines
            .next()
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|s| (Self::MIN..=Self::MAX).contains(s))
            .unwrap_or(16);
        Self { font, size }
    }
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::decode(&text))
            .unwrap_or_default()
    }
    fn save(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{}\n{}\n", self.font.key(), self.size))
    }
}

pub fn current(cx: &App) -> ReadingStyle {
    cx.try_global::<crate::settings::Preferences>()
        .map(|p| p.reading)
        .unwrap_or_default()
}

/// Changes notify all workspace windows. Slider drags preview immediately;
/// release writes the preference once instead of writing on every frame.
pub fn set(style: ReadingStyle, persist: bool, cx: &mut App) -> io::Result<()> {
    if !cx.has_global::<crate::settings::Preferences>() {
        crate::settings::init(cx);
    }
    let style = ReadingStyle {
        size: style.size.clamp(ReadingStyle::MIN, ReadingStyle::MAX),
        ..style
    };
    if current(cx) != style {
        cx.update_global::<crate::settings::Preferences, _>(|p, _| p.reading = style);
    }
    if persist {
        if let Some(path) = cx
            .global::<crate::settings::Preferences>()
            .reading_path
            .clone()
        {
            style.save(&path)?;
        }
    }
    Ok(())
}

pub fn preference_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|root| crate::files::app_folder(root).join("reading-style"))
}

pub fn style(view: TextView, cx: &App) -> TextView {
    let reading = current(cx);
    view.font_family(reading.font.family(cx))
        .text_size(px(reading.size as f32))
}

macro_rules! font_files {
    ($dir:literal, $family:literal) => {
        [
            include_bytes!(concat!(
                "../assets/fonts/",
                $dir,
                "/",
                $family,
                "-Regular.ttf"
            ))
            .as_slice(),
            include_bytes!(concat!(
                "../assets/fonts/",
                $dir,
                "/",
                $family,
                "-Italic.ttf"
            ))
            .as_slice(),
            include_bytes!(concat!("../assets/fonts/", $dir, "/", $family, "-Bold.ttf")).as_slice(),
            include_bytes!(concat!(
                "../assets/fonts/",
                $dir,
                "/",
                $family,
                "-BoldItalic.ttf"
            ))
            .as_slice(),
        ]
    };
}

pub struct FontAsset {
    pub file_name: String,
    pub data: &'static [u8],
    pub bold: bool,
    pub italic: bool,
}

impl ReadingFont {
    /// The exact, unmodified files used by the reader, PDF and HTML exports.
    pub fn assets(self) -> Vec<FontAsset> {
        let (name, files) = match self {
            Self::Default => return Vec::new(),
            Self::Inter => ("Inter", font_files!("inter", "Inter")),
            Self::Lora => ("Lora", font_files!("lora", "Lora")),
            Self::Mono => (
                "JetBrainsMono",
                font_files!("jetbrains-mono", "JetBrainsMono"),
            ),
            Self::Poppins => ("Poppins", font_files!("poppins", "Poppins")),
            Self::FiraCode => {
                return vec![
                    FontAsset {
                        file_name: "FiraCode-Regular.ttf".into(),
                        data: include_bytes!("../assets/fonts/fira-code/FiraCode-Regular.ttf"),
                        bold: false,
                        italic: false,
                    },
                    FontAsset {
                        file_name: "FiraCode-Bold.ttf".into(),
                        data: include_bytes!("../assets/fonts/fira-code/FiraCode-Bold.ttf"),
                        bold: true,
                        italic: false,
                    },
                ];
            }
        };
        ["Regular", "Italic", "Bold", "BoldItalic"]
            .into_iter()
            .zip(files)
            .enumerate()
            .map(|(i, (style, data))| FontAsset {
                file_name: format!("{name}-{style}.ttf"),
                data,
                bold: i >= 2,
                italic: i % 2 == 1,
            })
            .collect()
    }

    pub fn license(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Inter => include_str!("../assets/fonts/inter/OFL.txt"),
            Self::Lora => include_str!("../assets/fonts/lora/OFL.txt"),
            Self::Mono => include_str!("../assets/fonts/jetbrains-mono/OFL.txt"),
            Self::Poppins => include_str!("../assets/fonts/poppins/OFL.txt"),
            Self::FiraCode => include_str!("../assets/fonts/fira-code/OFL.txt"),
        }
    }
}
pub fn init(cx: &mut App) {
    let fonts = font_files!("inter", "Inter")
        .into_iter()
        .chain(font_files!("lora", "Lora"))
        .chain(font_files!("jetbrains-mono", "JetBrainsMono"))
        .chain(font_files!("poppins", "Poppins"))
        .chain([
            include_bytes!("../assets/fonts/fira-code/FiraCode-Regular.ttf").as_slice(),
            include_bytes!("../assets/fonts/fira-code/FiraCode-Bold.ttf").as_slice(),
        ])
        .map(Cow::Borrowed)
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("Could not load reading fonts: {error}");
    }
}

#[derive(Clone)]
struct FontOption {
    font: ReadingFont,
    name: SharedString,
}
impl SearchableListItem for FontOption {
    type Value = SharedString;
    fn title(&self) -> SharedString {
        self.name.clone()
    }
    fn value(&self) -> &SharedString {
        &self.name
    }
    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let font = self.font;
        div()
            .debug_selector(move || format!("font-option-{}", font.key()))
            .child(self.name.clone())
    }
}

pub struct FontPicker {
    select: Entity<SelectState<Vec<FontOption>>>,
    trigger_focus: FocusHandle,
    error: Option<String>,
    local_font: Option<ReadingFont>,
    _subscriptions: Vec<Subscription>,
}
impl FontPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::create(None, window, cx)
    }

    pub fn for_export(font: ReadingFont, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::create(Some(font), window, cx)
    }

    pub fn selected_font(&self, cx: &App) -> ReadingFont {
        self.local_font.unwrap_or_else(|| current(cx).font)
    }

    fn dropdown_has_focus(&self, window: &Window, cx: &App) -> bool {
        // Select exposes its list focus while open and its trigger otherwise.
        self.select.focus_handle(cx).is_focused(window) && !self.trigger_focus.is_focused(window)
    }

    fn create(
        local_font: Option<ReadingFont>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choices: Vec<FontOption> = ReadingFont::ALL
            .iter()
            .map(|font| FontOption {
                font: *font,
                name: font.name().into(),
            })
            .collect();
        let index = ReadingFont::ALL
            .iter()
            .position(|f| *f == local_font.unwrap_or_else(|| current(cx).font))
            .unwrap_or(0);
        let select =
            cx.new(|cx| SelectState::new(choices, Some(IndexPath::new(index)), window, cx));
        let change = cx.subscribe(&select, |this, _, event, cx| {
            let SelectEvent::Confirm(Some(name)) = event else {
                return;
            };
            if let Some(font) = ReadingFont::ALL
                .into_iter()
                .find(|f| f.name() == name.as_ref())
            {
                if this.local_font.is_some() {
                    this.local_font = Some(font);
                    cx.notify();
                    return;
                }
                this.error = set(
                    ReadingStyle {
                        font,
                        ..current(cx)
                    },
                    true,
                    cx,
                )
                .err()
                .map(|e| e.to_string());
                cx.notify();
            }
        });
        let prefs = cx.observe_global::<crate::settings::Preferences>(|_, cx| cx.notify());
        let trigger_focus = select.focus_handle(cx);
        Self {
            select,
            trigger_focus,
            error: None,
            local_font,
            _subscriptions: vec![change, prefs],
        }
    }
}
impl Render for FontPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name: SharedString = self.selected_font(cx).name().into();
        if self.select.read(cx).selected_value() != Some(&name) {
            self.select.update(cx, |select, cx| {
                select.set_selected_value(&name, window, cx)
            });
        }
        v_flex()
            .w_full()
            .gap_1()
            .child(
                div().debug_selector(|| "reading-font-select".into()).child(
                    Select::new(&self.select)
                        .small()
                        .w_full()
                        .menu_width(px(216.))
                        .accessibility_label("Font Style"),
                ),
            )
            .when_some(self.error.clone(), |view, error| {
                view.child(div().text_xs().text_color(cx.theme().danger).child(error))
            })
    }
}

/// A compact control shared by document menus. Owns the slider subscription so
/// it survives rerenders and keeps in sync with changes from another window.
pub struct ReadingControls {
    picker: Entity<FontPicker>,
    slider: Entity<SliderState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}
impl ReadingControls {
    pub fn font_dropdown_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.picker.read(cx).dropdown_has_focus(window, cx)
    }
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let reading = current(cx);
        let picker = cx.new(|cx| FontPicker::new(window, cx));
        let slider = cx.new(|_| {
            SliderState::new()
                .min(ReadingStyle::MIN as f32)
                .max(ReadingStyle::MAX as f32)
                .step(1.)
                .default_value(reading.size as f32)
        });
        let changes = cx.subscribe(&slider, |this, _, event, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            this.error = set(
                ReadingStyle {
                    size: value.start().round() as u8,
                    ..current(cx)
                },
                matches!(event, SliderEvent::Release(_)),
                cx,
            )
            .err()
            .map(|e| e.to_string());
            cx.notify();
        });
        let prefs = cx.observe_global::<crate::settings::Preferences>(|_, cx| cx.notify());
        Self {
            picker,
            slider,
            error: None,
            _subscriptions: vec![changes, prefs],
        }
    }
}
impl Render for ReadingControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let reading = current(cx);
        if self.slider.read(cx).value().start() != reading.size as f32 {
            self.slider
                .update(cx, |s, cx| s.set_value(reading.size as f32, window, cx));
        }
        v_flex()
            .w_full()
            .gap_3()
            // With the menu's own 4px, text sits 12px in, as the rows below.
            .p_2()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Font Style"),
                    )
                    .child(
                        Button::new("reset-reading-style")
                            .ghost()
                            .xsmall()
                            .label("Reset")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.error = set(ReadingStyle::default(), true, cx)
                                    .err()
                                    .map(|e| e.to_string());
                                cx.notify();
                            })),
                    ),
            )
            .child(self.picker.clone())
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.muted_foreground)
                            .child("A"),
                    )
                    .child(
                        div()
                            .id("reading-size-slider")
                            .debug_selector(|| "reading-size-slider".into())
                            .flex_1()
                            .child(Slider::new(&self.slider).accessibility_label("Font Size")),
                    )
                    .child(div().text_size(px(22.)).child("A")),
            )
            .when_some(self.error.clone(), |panel, error| {
                panel.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use super::{ReadingFont, ReadingStyle, current};
    use crate::{Enso, settings};
    use ::core::prelude::v1::test;
    use gpui_kit::{
        AppContext, BorrowAppContext, Bounds, Modifiers, TestAppContext, VisualTestContext,
        WindowBounds, WindowOptions, point, px, size,
    };

    #[gpui_kit::test]
    fn document_menu_changes_reading_style_and_saves_on_slider_release(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join(format!("enso-reading-menu-{}", std::process::id()));
        cx.update(|cx| {
            gpui_kit::init(cx);
            settings::init(cx);
            cx.update_global::<settings::Preferences, _>(|p, _| {
                p.reading = ReadingStyle::default();
                p.reading_path = Some(path.clone());
            });
        });
        let open = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(0.), px(0.)),
                            size(px(1000.), px(720.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        cx.new(|cx| {
                            let mut app = Enso::new(window, cx);
                            app.add_document(
                                None,
                                "# Heading\n\nReading text.".into(),
                                None,
                                window,
                                cx,
                            );
                            app
                        })
                    },
                )
                .unwrap()
            })
        };
        let (first, app) = open(cx);
        let (second, other) = open(cx);
        let draw = |cx: &mut TestAppContext| {
            for handle in [first, second] {
                cx.update_window(handle, |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .unwrap();
            }
            cx.run_until_parked();
        };
        draw(cx);
        let heading_height = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                other.read(cx).documents[0]
                    .preview
                    .read(cx)
                    .list_state()
                    .bounds_for_item(0)
                    .unwrap()
                    .size
                    .height
            })
        };
        let before = heading_height(cx);
        let mut visual = VisualTestContext::from_window(first, cx);
        let trigger = visual.debug_bounds("document-menu-trigger").unwrap();
        visual.simulate_click(trigger.center(), Modifiers::default());
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        let choice = visual
            .debug_bounds("reading-font-select")
            .expect("font dropdown");
        visual.simulate_click(choice.center(), Modifiers::default());
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        let option = visual
            .debug_bounds("font-option-fira-code")
            .expect("last font option");
        let menu = visual.debug_bounds("document-menu-panel").unwrap();
        assert!(
            option.center().y > menu.bottom(),
            "exercise the dropdown outside its parent surface"
        );
        visual.simulate_click(option.center(), Modifiers::default());
        draw(cx);
        assert_eq!(cx.update(|cx| current(cx).font), ReadingFont::FiraCode);
        let slider = visual
            .debug_bounds("reading-size-slider")
            .expect("slider stays open after font change");
        visual.simulate_click(
            point(slider.left() + slider.size.width * 0.8, slider.center().y),
            Modifiers::default(),
        );
        draw(cx);
        let saved = ReadingStyle::load(&path);
        assert_eq!(cx.update(|cx| current(cx)), saved);
        assert!(saved.size >= 23, "{saved:?}");
        assert!(
            heading_height(cx) > before,
            "other window's heading must reflow with the larger font"
        );
        cx.update(|cx| {
            for view in [&app, &other] {
                let doc = &view.read(cx).documents[0];
                assert_eq!(
                    doc.editor.read(cx).value().as_str(),
                    "# Heading\n\nReading text."
                );
            }
        });
        // A real outside click still dismisses both nested surfaces.
        let choice = visual.debug_bounds("reading-font-select").unwrap();
        visual.simulate_click(choice.center(), Modifiers::default());
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        assert!(visual.debug_bounds("font-option-fira-code").is_some());
        let menu = visual.debug_bounds("document-menu-panel").unwrap();
        visual.simulate_click(
            point(menu.left() - px(40.), menu.bottom() + px(40.)),
            Modifiers::default(),
        );
        draw(cx);
        assert!(visual.debug_bounds("document-menu-panel").is_none());
        assert!(visual.debug_bounds("font-option-fira-code").is_none());
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn reading_preferences_round_trip_and_reject_invalid_sizes() {
        let path = std::env::temp_dir().join(format!("enso-reading-{}", std::process::id()));
        for font in ReadingFont::ALL {
            let style = ReadingStyle { font, size: 21 };
            style.save(&path).unwrap();
            assert_eq!(ReadingStyle::load(&path), style);
        }
        std::fs::remove_file(&path).unwrap();
        assert_eq!(ReadingStyle::load(&path), ReadingStyle::default());
        for text in ["unknown\nNaN", "default\n255", "default\n0"] {
            assert_eq!(ReadingStyle::decode(text), ReadingStyle::default());
        }
    }
    #[::core::prelude::v1::test]
    fn bundled_fonts_include_regular_bold_and_italic_faces() {
        let mut db = fontdb::Database::new();
        for data in font_files!("inter", "Inter")
            .into_iter()
            .chain(font_files!("lora", "Lora"))
            .chain(font_files!("jetbrains-mono", "JetBrainsMono"))
            .chain(font_files!("poppins", "Poppins"))
            .chain([
                include_bytes!("../assets/fonts/fira-code/FiraCode-Regular.ttf").as_slice(),
                include_bytes!("../assets/fonts/fira-code/FiraCode-Bold.ttf").as_slice(),
            ])
        {
            db.load_font_data(data.to_vec());
        }
        for family in ["Inter", "Lora", "JetBrains Mono", "Poppins", "Fira Code"] {
            let faces: Vec<_> = db
                .faces()
                .filter(|face| face.families.iter().any(|(name, _)| name == family))
                .collect();
            assert_eq!(
                faces.len(),
                if family == "Fira Code" { 2 } else { 4 },
                "{family}"
            );
            // Upstream JetBrains Mono uses an optical weight of 558 for the
            // face named Bold; check the actual face rather than assuming 700.
            assert!(
                faces
                    .iter()
                    .any(|f| f.post_script_name.ends_with("-Bold") && f.weight.0 > 400),
                "{family}"
            );
            if family != "Fira Code" {
                assert!(faces.iter().any(|f| f.style == fontdb::Style::Italic));
            }
        }
    }
}
