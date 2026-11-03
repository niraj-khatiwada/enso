use gpui_kit::{App, Global, Hsla, rgb};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaretColor {
    #[default]
    Theme,
    Accent,
    Custom(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FocusMode {
    #[default]
    Off,
    Sentence,
    Paragraph,
}

impl FocusMode {
    pub const ALL: [FocusMode; 3] = [FocusMode::Off, FocusMode::Sentence, FocusMode::Paragraph];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Sentence => "sentence",
            Self::Paragraph => "paragraph",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Sentence => "Sentence",
            Self::Paragraph => "Paragraph",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "sentence" => Self::Sentence,
            "paragraph" => Self::Paragraph,
            _ => Self::Off,
        }
    }
}

pub const BLINK_CELLS: u32 = 12;
pub const MIN_TEXT: f32 = 13.;
pub const MAX_TEXT: f32 = 26.;

#[derive(Clone, PartialEq, Debug)]
pub struct WritingPrefs {
    pub caret_color: CaretColor,
    pub caret_width: f32,
    pub caret_blink: bool,
    pub blink_period_ms: u32,
    pub blink_pattern: u16,
    pub typewriter: bool,
    pub typewriter_guide: f32,
    pub focus: FocusMode,
    pub text_size: f32,
}

impl Default for WritingPrefs {
    fn default() -> Self {
        Self {
            caret_color: CaretColor::Theme,
            caret_width: 2.,
            caret_blink: true,
            blink_period_ms: 1060,
            blink_pattern: 0b0000_0011_1111,
            typewriter: false,
            typewriter_guide: 0.5,
            focus: FocusMode::Off,
            text_size: 17.,
        }
    }
}

impl Global for WritingPrefs {}

impl WritingPrefs {
    pub fn get(cx: &App) -> WritingPrefs {
        cx.try_global::<WritingPrefs>().cloned().unwrap_or_default()
    }

    pub fn shows_in(&self, cell: u32) -> bool {
        !self.caret_blink || self.blink_pattern & (1 << (cell % BLINK_CELLS)) != 0
    }

    pub fn caret_hsla(&self, theme_caret: Hsla, accent: Hsla) -> Hsla {
        match self.caret_color {
            CaretColor::Theme => theme_caret,
            CaretColor::Accent => accent,
            CaretColor::Custom(rgb_value) => rgb(rgb_value).into(),
        }
    }

    pub fn serialize(&self) -> String {
        let color = match self.caret_color {
            CaretColor::Theme => "theme".to_string(),
            CaretColor::Accent => "accent".to_string(),
            CaretColor::Custom(value) => format!("#{value:06x}"),
        };
        format!(
            "caret_color = {color}\ncaret_width = {}\ncaret_blink = {}\nblink_period_ms = {}\nblink_pattern = {:012b}\ntypewriter = {}\ntypewriter_guide = {:.3}\nfocus = {}\ntext_size = {}\n",
            self.caret_width,
            self.caret_blink,
            self.blink_period_ms,
            self.blink_pattern,
            self.typewriter,
            self.typewriter_guide,
            self.focus.name(),
            self.text_size,
        )
    }

    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "caret_color" => {
                    prefs.caret_color = match value {
                        "accent" => CaretColor::Accent,
                        v if v.starts_with('#') => u32::from_str_radix(&v[1..], 16)
                            .ok()
                            .filter(|v| *v <= 0xff_ffff)
                            .map_or(CaretColor::Theme, CaretColor::Custom),
                        _ => CaretColor::Theme,
                    }
                }
                "caret_width" => {
                    if let Ok(v) = value.parse::<f32>() {
                        prefs.caret_width = v.clamp(1., 4.);
                    }
                }
                "caret_blink" => prefs.caret_blink = value != "false",
                "blink_period_ms" => {
                    if let Ok(v) = value.parse::<u32>() {
                        prefs.blink_period_ms = v.clamp(300, 3000);
                    }
                }
                "blink_pattern" => {
                    if let Ok(v) = u16::from_str_radix(value, 2) {
                        let v = v & ((1 << BLINK_CELLS) - 1);
                        if v != 0 {
                            prefs.blink_pattern = v;
                        }
                    }
                }
                "typewriter" => prefs.typewriter = value == "true",
                "typewriter_guide" => {
                    if let Ok(v) = value.parse::<f32>() {
                        prefs.typewriter_guide = v.clamp(0.15, 0.85);
                    }
                }
                "focus" => prefs.focus = FocusMode::parse(value),
                "text_size" => {
                    if let Ok(v) = value.parse::<f32>() {
                        prefs.text_size = v.clamp(MIN_TEXT, MAX_TEXT);
                    }
                }
                _ => {}
            }
        }
        prefs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_tolerate_bad_values() {
        let prefs = WritingPrefs {
            caret_color: CaretColor::Custom(0x8839ef),
            caret_width: 3.,
            caret_blink: false,
            blink_period_ms: 800,
            blink_pattern: 0b0000_1111_1111,
            typewriter: true,
            typewriter_guide: 0.4,
            focus: FocusMode::Sentence,
            text_size: 19.,
        };
        assert_eq!(WritingPrefs::parse(&prefs.serialize()), prefs);
        let odd = WritingPrefs::parse("caret_width = 99\nblink_pattern = 0\nfocus = ??\nnoise\n");
        assert_eq!(odd.caret_width, 4.);
        assert_eq!(odd.blink_pattern, WritingPrefs::default().blink_pattern);
        assert_eq!(odd.focus, FocusMode::Off);
    }

    #[test]
    fn blink_pattern_steps() {
        let prefs = WritingPrefs::default();
        let shown: Vec<bool> = (0..BLINK_CELLS).map(|c| prefs.shows_in(c)).collect();
        assert_eq!(shown.iter().filter(|s| **s).count(), 6);
        assert!(shown[0] && !shown[11]);
        let solid = WritingPrefs {
            caret_blink: false,
            ..prefs
        };
        assert!((0..BLINK_CELLS).all(|c| solid.shows_in(c)));
    }
}
