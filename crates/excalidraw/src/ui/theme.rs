use crate::core::types::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

impl From<Theme> for ThemeMode {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Self::Light,
            Theme::Dark => Self::Dark,
        }
    }
}

impl From<ThemeMode> for Theme {
    fn from(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Theme::Dark,
            _ => Theme::Light,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    pub fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasColors {
    pub background: &'static str,
    pub grid: &'static str,
    pub stroke: &'static str,
}

pub fn canvas_colors(theme: AppTheme) -> CanvasColors {
    match theme {
        AppTheme::Light => CanvasColors {
            background: "#ffffff",
            grid: "#e5e5e5",
            stroke: "#1e1e1e",
        },
        AppTheme::Dark => CanvasColors {
            background: "#121212",
            grid: "#2c2c2c",
            stroke: "#e0e0e0",
        },
    }
}

pub struct PaletteEntry {
    pub name: &'static str,
    pub shades: &'static [&'static str],
}

impl PaletteEntry {
    pub fn at(&self, shade: usize) -> &'static str {
        if self.shades.is_empty() {
            return "transparent";
        }
        self.shades[shade.min(self.shades.len() - 1)]
    }

    pub fn is_ramp(&self) -> bool {
        self.shades.len() > 1
    }
}

const TRANSPARENT: &[&str] = &["transparent"];
const BLACK: &[&str] = &["#1e1e1e"];
const WHITE: &[&str] = &["#ffffff"];
const GRAY: &[&str] = &["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"];
const RED: &[&str] = &["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"];
const PINK: &[&str] = &["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"];
const GRAPE: &[&str] = &["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"];
const VIOLET: &[&str] = &["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"];
const BLUE: &[&str] = &["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"];
const CYAN: &[&str] = &["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"];
const TEAL: &[&str] = &["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"];
const GREEN: &[&str] = &["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"];
const YELLOW: &[&str] = &["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"];
const ORANGE: &[&str] = &["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"];
const BRONZE: &[&str] = &["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"];

pub fn element_palette() -> &'static [PaletteEntry] {
    &[
        PaletteEntry {
            name: "transparent",
            shades: TRANSPARENT,
        },
        PaletteEntry {
            name: "white",
            shades: WHITE,
        },
        PaletteEntry {
            name: "gray",
            shades: GRAY,
        },
        PaletteEntry {
            name: "black",
            shades: BLACK,
        },
        PaletteEntry {
            name: "bronze",
            shades: BRONZE,
        },
        PaletteEntry {
            name: "cyan",
            shades: CYAN,
        },
        PaletteEntry {
            name: "blue",
            shades: BLUE,
        },
        PaletteEntry {
            name: "violet",
            shades: VIOLET,
        },
        PaletteEntry {
            name: "grape",
            shades: GRAPE,
        },
        PaletteEntry {
            name: "pink",
            shades: PINK,
        },
        PaletteEntry {
            name: "green",
            shades: GREEN,
        },
        PaletteEntry {
            name: "teal",
            shades: TEAL,
        },
        PaletteEntry {
            name: "yellow",
            shades: YELLOW,
        },
        PaletteEntry {
            name: "orange",
            shades: ORANGE,
        },
        PaletteEntry {
            name: "red",
            shades: RED,
        },
    ]
}

pub const DEFAULT_STROKE_SHADE: usize = 4;

pub const DEFAULT_BACKGROUND_SHADE: usize = 1;

pub const STROKE_TOP_PICKS: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];

pub const BACKGROUND_TOP_PICKS: [&str; 5] =
    ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/// Upstream's bucket fill picks: the background picks with `transparent`, a
/// no-op fill for the bucket, swapped for white.
pub const BUCKET_FILL_TOP_PICKS: [&str; 5] =
    ["#ffffff", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

pub fn is_custom_color(color: &str) -> bool {
    !element_palette()
        .iter()
        .any(|entry| entry.shades.contains(&color))
}

pub fn color_position(color: &str) -> Option<(&'static str, Option<usize>)> {
    for entry in element_palette() {
        if let Some(shade) = entry.shades.iter().position(|c| *c == color) {
            return Some((entry.name, if entry.is_ramp() { Some(shade) } else { None }));
        }
    }
    None
}
