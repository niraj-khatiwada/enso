use std::{collections::HashMap, fmt::Write, sync::OnceLock};

use base64::Engine as _;

use crate::{
    core::{
        bounds::{common_bounds, element_bounds},
        element::Element,
        scene::{BinaryFileData, Scene},
        types::{FillStyle, FontFamily},
    },
    render::shape::{Drawable, Op, Rgba, render_element, render_element_with_lookup},
};

pub type BinaryFiles = HashMap<String, BinaryFileData>;

pub const DEFAULT_EXPORT_PADDING: f64 = 10.0;

/// PNG `tEXt` keyword upstream uses to carry the scene payload.
pub const PNG_SCENE_KEYWORD: &str = "application/vnd.excalidraw+json";

const DARK_MODE_INVERT_PERCENT: f64 = 93.0;
const DARK_MODE_HUE_ROTATE_DEGREES: f64 = 180.0;

/// Upstream darkens exports with the CSS filter `invert(93%) hue-rotate(180deg)`
/// rather than swapping palettes, so a yellow sticky note comes out dark olive
/// and a white background comes out #121212. Replicating the same maths keeps our
/// SVG and PNG pixel-identical to theirs.
pub fn dark_mode_color(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let percent = DARK_MODE_INVERT_PERCENT / 100.0;
    let invert = |channel: f64| {
        (channel * (1.0 - percent) + (255.0 - channel) * percent)
            .round()
            .clamp(0.0, 255.0)
    };
    let (r, g, b) = (
        invert(r as f64) / 255.0,
        invert(g as f64) / 255.0,
        invert(b as f64) / 255.0,
    );

    let radians = DARK_MODE_HUE_ROTATE_DEGREES.to_radians();
    let (cos, sin) = (radians.cos(), radians.sin());
    let m = [
        0.213 + cos * 0.787 - sin * 0.213,
        0.715 - cos * 0.715 - sin * 0.715,
        0.072 - cos * 0.072 + sin * 0.928,
        0.213 - cos * 0.213 + sin * 0.143,
        0.715 + cos * 0.285 + sin * 0.140,
        0.072 - cos * 0.072 - sin * 0.283,
        0.213 - cos * 0.213 - sin * 0.787,
        0.715 - cos * 0.715 + sin * 0.715,
        0.072 + cos * 0.928 + sin * 0.072,
    ];
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    (
        channel(r * m[0] + g * m[1] + b * m[2]),
        channel(r * m[3] + g * m[4] + b * m[5]),
        channel(r * m[6] + g * m[7] + b * m[8]),
    )
}

/// The same filter applied to a `#rrggbb` string, used for the export background
/// and by the Mermaid preview when the dark theme is active.
pub(crate) fn dark_mode_hex(color: &str) -> String {
    let rgba = Rgba::from_hex(color);
    let (r, g, b) = dark_mode_color(
        (rgba.r * 255.0).round() as u8,
        (rgba.g * 255.0).round() as u8,
        (rgba.b * 255.0).round() as u8,
    );
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Attach the scene JSON to an encoded PNG as a `tEXt` chunk placed right
/// before `IEND`, which is where readers expect to find it.
pub fn embed_png_scene_data(png: &[u8], scene_json: &str) -> Vec<u8> {
    if png.len() < 12 {
        return png.to_vec();
    }
    let mut data = Vec::with_capacity(PNG_SCENE_KEYWORD.len() + 1 + scene_json.len());
    data.extend_from_slice(PNG_SCENE_KEYWORD.as_bytes());
    data.push(0);
    data.extend_from_slice(scene_json.as_bytes());

    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(&data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(b"tEXt");
    crc_input.extend_from_slice(&data);
    chunk.extend_from_slice(&crc32(&crc_input).to_be_bytes());

    // The last 12 bytes are the (empty) IEND chunk; splice ours in ahead of it.
    let split = png.len() - 12;
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..split]);
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[split..]);
    out
}

/// Attach the scene JSON to an SVG document as a `<metadata>` element.
pub fn embed_svg_scene_data(svg: &str, scene_json: &str) -> String {
    let meta = format!(
        "<metadata id=\"excalidraw-scene\">{}</metadata>",
        escape_xml(scene_json)
    );
    match svg.rfind("</svg>") {
        Some(index) => format!("{}{meta}{}", &svg[..index], &svg[index..]),
        None => svg.to_string(),
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

pub struct SvgExporter;

pub fn rasterize_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let tree = usvg::Tree::from_str(svg, svg_options()).map_err(|e| e.to_string())?;
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = (tree.size().width() * scale).ceil().max(1.0) as u32;
    let height = (tree.size().height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("invalid pixmap size {width}x{height}"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

pub fn rasterize_svg(svg: &[u8], scale: f32) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(svg).map_err(|e| e.to_string())?;
    rasterize_png(text, scale)
}

pub fn export_png(scene: &Scene, background: &str, scale: f32) -> Result<Vec<u8>, String> {
    export_png_with_files(scene, background, scale, &BinaryFiles::new())
}

pub fn export_png_with_files(
    scene: &Scene,
    background: &str,
    scale: f32,
    files: &BinaryFiles,
) -> Result<Vec<u8>, String> {
    export_png_with_options(scene, background, scale, files, false)
}

pub fn export_png_with_options(
    scene: &Scene,
    background: &str,
    scale: f32,
    files: &BinaryFiles,
    dark_mode: bool,
) -> Result<Vec<u8>, String> {
    rasterize_png(
        &SvgExporter::export_with_options(scene, background, files, dark_mode),
        scale,
    )
}

fn svg_options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options::default();
        {
            // Register the hand-drawn faces so exported text rasterizes with the same
            // letterforms the editor draws, instead of falling back to a sans-serif.
            let db = options.fontdb_mut();
            db.load_system_fonts();
            db.load_font_data(crate::ui::fonts::EXCALIFONT_TTF.to_vec());
            db.load_font_data(crate::ui::fonts::VIRGIL_TTF.to_vec());
        }
        options
    })
}

fn font_family_name(family: FontFamily) -> &'static str {
    match family {
        FontFamily::Virgil => "Virgil, sans-serif",
        FontFamily::Helvetica => "Helvetica, sans-serif",
        FontFamily::Cascadia => "Cascadia, monospace",
        FontFamily::Assistant => "Assistant, sans-serif",
        FontFamily::Excalifont => "Excalifont, sans-serif",
        FontFamily::ComicShanns => "ComicShanns, cursive",
        FontFamily::Liberation => "Liberation Sans, sans-serif",
        FontFamily::Nunito => "Nunito, sans-serif",
        FontFamily::Lilita => "Lilita One, sans-serif",
        FontFamily::Xiaolai => "Xiaolai, sans-serif",
    }
}

fn embedded_font_face_css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        format!(
            "<defs><style>@font-face{{font-family:'Excalifont';src:url(data:font/ttf;base64,{}) format('truetype');}}@font-face{{font-family:'Virgil';src:url(data:font/ttf;base64,{}) format('truetype');}}</style></defs>",
            encode(crate::ui::fonts::EXCALIFONT_TTF),
            encode(crate::ui::fonts::VIRGIL_TTF),
        )
    })
}

impl SvgExporter {
    pub fn export(scene: &Scene, background: &str) -> String {
        Self::export_with_files(scene, background, &BinaryFiles::new())
    }

    pub fn export_with_files(scene: &Scene, background: &str, files: &BinaryFiles) -> String {
        Self::export_with_options(scene, background, files, false)
    }

    pub fn export_with_options(
        scene: &Scene,
        background: &str,
        files: &BinaryFiles,
        dark_mode: bool,
    ) -> String {
        let elements: Vec<&Element> = scene.non_deleted().collect();
        let bounds =
            common_bounds(&elements).unwrap_or(crate::core::Bounds::new(0.0, 0.0, 100.0, 100.0));
        let padding = DEFAULT_EXPORT_PADDING;
        let width = bounds.width() + padding * 2.0;
        let height = bounds.height() + padding * 2.0;
        let ox = bounds.min_x - padding;
        let oy = bounds.min_y - padding;

        let mut svg = String::new();
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.1}\" height=\"{:.1}\" viewBox=\"{:.1} {:.1} {:.1} {:.1}\">",
            width, height, ox, oy, width, height
        );
        svg.push_str(embedded_font_face_css());

        if !crate::core::color::is_transparent(background) {
            let fill = if dark_mode {
                dark_mode_hex(background)
            } else {
                background.to_string()
            };
            let _ = write!(
                svg,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>",
                ox, oy, width, height, fill
            );
        }

        let lookup: HashMap<&str, &Element> = elements.iter().map(|e| (e.id(), *e)).collect();
        for element in &elements {
            let drawables = render_element_with_lookup(element, &lookup);
            for d in drawables {
                write_drawable(&mut svg, &d, files, dark_mode);
            }
        }

        svg.push_str("</svg>");
        svg
    }

    pub fn export_element(element: &Element, background: &str) -> String {
        Self::export_element_with_files(element, background, &BinaryFiles::new())
    }

    pub fn export_element_with_files(
        element: &Element,
        background: &str,
        files: &BinaryFiles,
    ) -> String {
        let b = element_bounds(element);
        let padding = DEFAULT_EXPORT_PADDING;
        let width = b.width() + padding * 2.0;
        let height = b.height() + padding * 2.0;
        let ox = b.min_x - padding;
        let oy = b.min_y - padding;

        let mut svg = String::new();
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.1}\" height=\"{:.1}\" viewBox=\"{:.1} {:.1} {:.1} {:.1}\">",
            width, height, ox, oy, width, height
        );
        svg.push_str(embedded_font_face_css());
        if !crate::core::color::is_transparent(background) {
            let _ = write!(
                svg,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>",
                ox, oy, width, height, background
            );
        }
        for d in render_element(element) {
            write_drawable(&mut svg, &d, files, false);
        }
        svg.push_str("</svg>");
        svg
    }
}

fn write_drawable(svg: &mut String, d: &Drawable, files: &BinaryFiles, dark_mode: bool) {
    if let Some(t) = &d.text {
        let color = rgba_attr(t.color.r, t.color.g, t.color.b, t.color.a, dark_mode);

        let (anchor, anchor_x) = match t.text_align {
            crate::core::types::TextAlign::Left => ("start", t.x),
            crate::core::types::TextAlign::Center => ("middle", t.x + t.width / 2.0),
            crate::core::types::TextAlign::Right => ("end", t.x + t.width),
        };
        let font_family = font_family_name(t.font_family);

        let lines: Vec<&str> = t.text.split('\n').collect();
        let line_h = t.font_size * t.line_height;
        let total_h = line_h * lines.len() as f64;
        let block_top = match t.vertical_align {
            crate::core::types::VerticalAlign::Top => t.y,
            crate::core::types::VerticalAlign::Middle => t.y + (t.height - total_h) / 2.0,
            crate::core::types::VerticalAlign::Bottom => t.y + t.height - total_h,
        };
        let _ = write!(
            svg,
            "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" font-family=\"{}\" fill=\"{}\" text-anchor=\"{}\" dominant-baseline=\"middle\">",
            anchor_x,
            block_top + line_h / 2.0,
            t.font_size,
            font_family,
            color,
            anchor
        );
        for (i, line) in lines.iter().enumerate() {
            let y = block_top + (i as f64 + 0.5) * line_h;
            let _ = write!(
                svg,
                "<tspan x=\"{:.2}\" y=\"{:.2}\">{}</tspan>",
                anchor_x,
                y,
                escape_xml(line)
            );
        }
        svg.push_str("</text>");
        return;
    }

    if let Some(img) = &d.image {
        if let Some(data) = img.file_id.as_deref().and_then(|id| files.get(id)) {
            let _ = write!(
                svg,
                "<image x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"{}\"/>",
                img.x,
                img.y,
                img.width,
                img.height,
                escape_xml(&data.data_url)
            );
            return;
        }
        let _ = write!(
            svg,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#d0d0d0\" stroke=\"#868e96\" stroke-width=\"1\"/>",
            img.x, img.y, img.width, img.height
        );
        let _ = write!(
            svg,
            "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"12\" fill=\"#495057\" text-anchor=\"middle\">image</text>",
            img.x + img.width / 2.0,
            img.y + img.height / 2.0
        );
        return;
    }

    if let Some(ink) = &d.ink {
        let path = ops_to_path(ink);
        let fill = rgba_attr(
            d.options.stroke_color.r,
            d.options.stroke_color.g,
            d.options.stroke_color.b,
            d.options.stroke_color.a,
            dark_mode,
        );
        let _ = write!(
            svg,
            "<path d=\"{}\" fill=\"{}\" stroke=\"none\" fill-rule=\"nonzero\"/>",
            path, fill
        );
        return;
    }

    // Solid fills are painted from the element's outline rather than from the rough
    // stroke sets: filling the stroked path would shut out the double-drawn outline
    // and drop the background colour entirely.
    if d.options.fill_style == FillStyle::Solid
        && let (Some(fill_path), Some(c)) = (&d.fill_path, d.options.fill_color)
    {
        let path = ops_to_path(fill_path);
        let _ = write!(
            svg,
            "<path d=\"{}\" fill=\"{}\" stroke=\"none\"/>",
            path,
            rgba_attr(c.r, c.g, c.b, c.a, dark_mode)
        );
    }

    for set in &d.sets {
        let path = ops_to_path(set);
        let _ = write!(
            svg,
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            path,
            rgba_attr(
                d.options.stroke_color.r,
                d.options.stroke_color.g,
                d.options.stroke_color.b,
                d.options.stroke_color.a,
                dark_mode,
            ),
            d.options.stroke_width
        );
    }
}

fn rgba_attr(r: f64, g: f64, b: f64, a: f64, dark_mode: bool) -> String {
    let channels = ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
    let (r, g, b) = if dark_mode {
        dark_mode_color(channels.0, channels.1, channels.2)
    } else {
        channels
    };
    format!("rgba({},{},{},{})", r, g, b, a)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn ops_to_path(ops: &[Op]) -> String {
    let mut path = String::new();
    for op in ops {
        match op {
            Op::MoveTo(x, y) => {
                let _ = write!(path, "M{:.2} {:.2} ", x, y);
            }
            Op::LineTo(x, y) => {
                let _ = write!(path, "L{:.2} {:.2} ", x, y);
            }
            Op::QuadraticTo(cx, cy, x, y) => {
                let _ = write!(path, "Q{:.2} {:.2} {:.2} {:.2} ", cx, cy, x, y);
            }
            Op::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                let _ = write!(
                    path,
                    "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
                    c1x, c1y, c2x, c2y, x, y
                );
            }
            Op::Close => path.push('Z'),
        }
    }
    path.trim_end().to_string()
}
