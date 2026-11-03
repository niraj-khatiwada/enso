use gpui_kit::base::text::editor_host::Paragraph;
use gpui_kit::{AbsoluteLength, DefiniteLength};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageAlign {
    #[default]
    Inline,
    Left,
    Center,
    Right,
}

impl ImageAlign {
    pub const ALL: [ImageAlign; 4] = [
        ImageAlign::Inline,
        ImageAlign::Left,
        ImageAlign::Center,
        ImageAlign::Right,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Inline => "Default",
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageBlock {
    pub url: String,
    pub alt: String,
    pub title: Option<String>,
    pub width: Option<u32>,
    pub align: ImageAlign,
}

impl ImageBlock {
    pub fn needs_html(&self) -> bool {
        self.width.is_some() || matches!(self.align, ImageAlign::Center | ImageAlign::Right)
    }

    pub fn markup(&self) -> String {
        if !self.needs_html() {
            let title = self
                .title
                .as_ref()
                .filter(|t| !t.is_empty())
                .map(|t| format!(" \"{}\"", t.replace('"', "\\\"")))
                .unwrap_or_default();
            return format!(
                "![{}]({}{})",
                self.alt.replace(']', "\\]"),
                markdown_url(&self.url),
                title
            );
        }
        let mut tag = format!(
            "<img src=\"{}\" alt=\"{}\"",
            attr(&self.url),
            attr(&self.alt)
        );
        if let Some(width) = self.width {
            tag.push_str(&format!(" width=\"{width}\""));
        }
        if self.align == ImageAlign::Right {
            tag.push_str(" align=\"right\"");
        }
        tag.push('>');
        if self.align == ImageAlign::Center {
            format!("<p align=\"center\">{tag}</p>")
        } else {
            tag
        }
    }
}

fn attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn markdown_url(url: &str) -> String {
    if url.contains(' ') || url.contains(')') {
        format!("<{url}>")
    } else {
        url.to_string()
    }
}

fn pixels(length: Option<DefiniteLength>) -> Option<u32> {
    match length? {
        DefiniteLength::Absolute(AbsoluteLength::Pixels(px)) => Some(f32::from(px).round() as u32),
        _ => None,
    }
}

fn align_in(source: &str) -> ImageAlign {
    let lower = source.to_ascii_lowercase();
    if !lower.trim_start().starts_with('<') {
        return ImageAlign::Inline;
    }
    let value = |key: &str| {
        lower.find(key).and_then(|at| {
            let rest = lower[at + key.len()..].trim_start_matches(['"', '\'']);
            ["left", "center", "right"]
                .into_iter()
                .find(|v| rest.starts_with(v))
        })
    };
    match value("align=") {
        Some("center") => ImageAlign::Center,
        Some("right") => ImageAlign::Right,
        Some("left") => ImageAlign::Left,
        _ if lower.contains("text-align:center") || lower.contains("text-align: center") => {
            ImageAlign::Center
        }
        _ => ImageAlign::Inline,
    }
}

pub fn lone_image(paragraph: &Paragraph, source: &str) -> Option<ImageBlock> {
    let mut found = None;
    for node in paragraph.children() {
        if let Some(image) = node.as_image() {
            if found.is_some() {
                return None;
            }
            found = Some(image.clone());
        } else if !node.text().trim().is_empty() || node.as_custom().is_some() {
            return None;
        }
    }
    let image = found?;
    let text = source.trim();
    if text.starts_with('[') {
        return None;
    }
    Some(ImageBlock {
        url: image.url.to_string(),
        alt: image.alt.map(|a| a.to_string()).unwrap_or_default(),
        title: image.title.map(|t| t.to_string()),
        width: pixels(image.width),
        align: align_in(text),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: Option<u32>, align: ImageAlign) -> ImageBlock {
        ImageBlock {
            url: "pics/a b.png".into(),
            alt: "A \"view\"".into(),
            title: None,
            width,
            align,
        }
    }

    #[test]
    fn markdown_unless_size_or_alignment_needs_html() {
        assert_eq!(
            image(None, ImageAlign::Inline).markup(),
            "![A \"view\"](<pics/a b.png>)"
        );
        assert_eq!(
            image(Some(320), ImageAlign::Inline).markup(),
            "<img src=\"pics/a b.png\" alt=\"A &quot;view&quot;\" width=\"320\">"
        );
        assert_eq!(
            image(None, ImageAlign::Center).markup(),
            "<p align=\"center\"><img src=\"pics/a b.png\" alt=\"A &quot;view&quot;\"></p>"
        );
        assert_eq!(
            image(Some(200), ImageAlign::Right).markup(),
            "<img src=\"pics/a b.png\" alt=\"A &quot;view&quot;\" width=\"200\" align=\"right\">"
        );
    }

    #[test]
    fn alignment_comes_from_html() {
        assert_eq!(
            align_in("<p align=\"center\"><img src=x></p>"),
            ImageAlign::Center
        );
        assert_eq!(align_in("<img src='x' align='right'>"), ImageAlign::Right);
        assert_eq!(align_in("![x](y)"), ImageAlign::Inline);
    }
}
