pub fn char_advance(ch: char, font_size: f64) -> f64 {
    if is_wide(ch) {
        font_size
    } else {
        font_size * 0.6
    }
}

fn is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD
    )
}

fn measure(text: &str, font_size: f64) -> f64 {
    text.chars().map(|c| char_advance(c, font_size)).sum()
}

/// Width of the widest line, used to size labels without a text layout pass.
pub fn text_width(text: &str, font_size: f64) -> f64 {
    text.split('\n')
        .map(|line| measure(line, font_size))
        .fold(0.0, f64::max)
}

/// Height of `text` for the given line height.
pub fn text_height(text: &str, font_size: f64, line_height: f64) -> f64 {
    text.split('\n').count().max(1) as f64 * font_size * line_height
}

pub fn wrap_text(text: &str, max_width: f64, font_size: f64) -> String {
    if !max_width.is_finite() || max_width <= 0.0 || font_size <= 0.0 {
        return text.to_string();
    }
    let mut lines: Vec<String> = Vec::new();
    for hard_line in text.split('\n') {
        lines.extend(wrap_line(hard_line, max_width, font_size));
    }
    lines.join("\n")
}

fn wrap_line(line: &str, max_width: f64, font_size: f64) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current: Vec<char> = Vec::new();
    let mut current_width = 0.0;
    let mut last_space: Option<usize> = None;

    for ch in line.chars() {
        let advance = char_advance(ch, font_size);

        if !ch.is_whitespace() && !current.is_empty() && current_width + advance > max_width {
            if let Some(space) = last_space.filter(|space| *space > 0) {
                let head: String = current[..space].iter().collect();
                let tail: String = current[space + 1..].iter().collect();
                lines.push(head.trim_end().to_string());
                current = tail.chars().collect();
                current_width = measure(&tail, font_size);
                last_space = None;
            } else {
                lines.push(current.iter().collect());
                current.clear();
                current_width = 0.0;
            }
        }
        if ch == ' ' {
            last_space = Some(current.len());
        }
        current.push(ch);
        current_width += advance;
    }

    let tail: String = current.iter().collect();
    let tail = tail.trim_end();
    if !tail.is_empty() || lines.is_empty() {
        lines.push(tail.to_string());
    }
    lines
}
