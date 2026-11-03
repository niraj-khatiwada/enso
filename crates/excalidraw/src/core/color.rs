use crate::types::Theme;

pub fn is_transparent(color: &str) -> bool {
    color == "transparent" || color == "none" || color.is_empty()
}

pub fn is_dark_color(hex: &str) -> bool {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return false;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f64;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f64;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f64;
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    luminance < 140.0
}

pub fn contrast_text_color(bg: &str) -> &'static str {
    if is_transparent(bg) {
        return "#1e1e1e";
    }
    if is_dark_color(bg) {
        "#ffffff"
    } else {
        "#1e1e1e"
    }
}

pub fn default_stroke_color(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "#1e1e1e",
        Theme::Dark => "#e0e0e0",
    }
}

pub fn default_background_color(_theme: Theme) -> &'static str {
    "transparent"
}
