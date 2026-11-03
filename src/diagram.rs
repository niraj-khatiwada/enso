//! Mermaid diagrams: a ```mermaid fence drawn as SVG by `bezel-mermaid` (a
//! pure-Rust port of Obsidian's Pele), in enso's palette. The reader shows
//! it as an image; exports embed the same SVG (HTML), or a picture of it
//! (PDF, rich text).

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

/// A drawn diagram.
#[derive(Clone)]
pub struct Diagram {
    pub svg: Arc<str>,
    /// In px, as drawn.
    pub width: f32,
    pub height: f32,
}

/// What went wrong drawing one, and where.
#[derive(Clone, Debug)]
pub struct Problem {
    pub message: String,
    pub line: usize,
}

/// The widest the reader shows a diagram; wider ones lay themselves out
/// narrower (running down instead of across) where their type can.
const MAX_WIDTH: f64 = 720.;

/// Is this fence's language Mermaid?
pub fn is_mermaid(lang: Option<&str>) -> bool {
    lang.is_some_and(|l| l.trim().eq_ignore_ascii_case("mermaid"))
}

/// Draw `source`, in the dark palette or the light one. Results are kept,
/// so a diagram is drawn again only when its source or the theme changes.
pub fn render(source: &str, dark: bool) -> Result<Diagram, Problem> {
    static CACHE: OnceLock<Mutex<HashMap<(u64, bool), Result<Diagram, Problem>>>> = OnceLock::new();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), dark);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(done) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return done;
    }
    let options = mermaid::Options {
        // Labels are measured with Arial's advances (`Metrics`), so they're
        // drawn in Arial (or a metric twin) to fit their boxes.
        // Unquoted: the list goes inside a `style` attribute, where a quote
        // would end it early (and leave the text in a serif fallback).
        font_family: Some("Arial, Helvetica, Liberation Sans, sans-serif".into()),
        font_size: 14.,
        max_width: Some(MAX_WIDTH),
        colors: Some(if dark { DARK } else { LIGHT }),
        links: false,
        images: false,
        ..Default::default()
    };
    let result = mermaid::render(source, &options, &mermaid::Metrics)
        .map(|drawing| Diagram {
            svg: drawing.svg.into(),
            width: drawing.width as f32,
            height: drawing.height as f32,
        })
        .map_err(|error| Problem { message: error.message.clone(), line: error.line });
    if let Ok(mut cache) = cache.lock() {
        // A note being typed into draws many versions; keep the recent ones.
        if cache.len() > 128 {
            cache.clear();
        }
        cache.insert(key, result.clone());
    }
    result
}

/// A picture of a drawn diagram: PNG bytes, `scale` times its size, on its
/// light panel colour. For PDFs and the clipboard, which take pictures more
/// readily than SVG.
pub fn png(diagram: &Diagram, scale: f32) -> Option<Vec<u8>> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    let options = resvg::usvg::Options {
        fontdb: fonts.clone(),
        // Not the serif usvg falls back to: the labels were measured in Arial.
        font_family: "Arial".into(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&diagram.svg, &options).ok()?;
    let size = tree.size();
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let [r, g, b, a] = LIGHT.bg;
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// The panel a diagram sits on, behind its drawing.
pub fn background(dark: bool) -> [u8; 4] {
    if dark { DARK.bg } else { LIGHT.bg }
}

const LIGHT: mermaid::Colors = mermaid::Colors {
    bg: [0xfa, 0xfb, 0xfc, 0xff],
    fg: [0x1f, 0x23, 0x28, 0xff],
    muted: [0x59, 0x63, 0x6e, 0xff],
    line: [0x57, 0x60, 0x6a, 0xff],
    surface: [0xff, 0xff, 0xff, 0xff],
    surface_alt: [0xef, 0xf2, 0xf5, 0xff],
    border: [0xc8, 0xd0, 0xd8, 0xff],
    accent: [0x09, 0x69, 0xda, 0xff],
    critical: [0xe4, 0x57, 0x56, 0xff],
    series: [
        [0x5b, 0x7b, 0xd5, 0xff],
        [0xf2, 0x8e, 0x2b, 0xff],
        [0x59, 0xa1, 0x4f, 0xff],
        [0xe1, 0x57, 0x59, 0xff],
        [0x76, 0xb7, 0xb2, 0xff],
        [0xed, 0xc9, 0x48, 0xff],
        [0xb0, 0x7a, 0xa1, 0xff],
        [0xff, 0x9d, 0xa7, 0xff],
    ],
};

const DARK: mermaid::Colors = mermaid::Colors {
    bg: [0x1c, 0x1d, 0x1f, 0xff],
    fg: [0xe6, 0xe6, 0xe6, 0xff],
    muted: [0x9a, 0x9f, 0xa6, 0xff],
    line: [0xaa, 0xb0, 0xb6, 0xff],
    surface: [0x26, 0x28, 0x2b, 0xff],
    surface_alt: [0x30, 0x33, 0x36, 0xff],
    border: [0x4c, 0x52, 0x58, 0xff],
    accent: [0x58, 0xa6, 0xff, 0xff],
    critical: [0xf8, 0x71, 0x71, 0xff],
    series: [
        [0x7c, 0x9c, 0xf0, 0xff],
        [0xf5, 0xa1, 0x4f, 0xff],
        [0x75, 0xc0, 0x6b, 0xff],
        [0xf0, 0x75, 0x77, 0xff],
        [0x8f, 0xd0, 0xcb, 0xff],
        [0xf2, 0xd3, 0x5f, 0xff],
        [0xc9, 0x96, 0xbb, 0xff],
        [0xff, 0xb3, 0xbb, 0xff],
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowcharts_draw_and_mistakes_say_where() {
        let drawn = render("flowchart LR\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Go]", false).unwrap();
        assert!(drawn.svg.starts_with("<svg") && drawn.width > 0. && drawn.height > 0.);
        // No CSS variables left: the palette is written in.
        assert!(!drawn.svg.contains("var(--"), "{}", &drawn.svg[..200.min(drawn.svg.len())]);
        // The font list survives intact inside its `style` attribute.
        assert!(drawn.svg.contains("font-family:Arial, Helvetica, Liberation Sans, sans-serif"));
        let problem = render("flowchart LR\n  A --> ", false).err().unwrap();
        assert!(!problem.message.is_empty());
        assert!(is_mermaid(Some("Mermaid")) && !is_mermaid(Some("rust")) && !is_mermaid(None));
        // Pictures come out at the asked scale, as PNG.
        let picture = png(&drawn, 2.).unwrap();
        assert!(picture.starts_with(b"\x89PNG"));
        let decoded = image::load_from_memory(&picture).unwrap();
        assert_eq!(decoded.width(), (drawn.width * 2.).ceil() as u32);
    }
}

