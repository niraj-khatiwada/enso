use std::sync::Arc;

use base64::Engine as _;
use gpui_kit::*;

pub fn base64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{};base64,{}", mime, base64_encode(bytes))
}

pub fn bytes_from_data_url(data_url: &str) -> Option<Vec<u8>> {
    let (_, encoded) = data_url.split_once("base64,")?;
    base64_decode(encoded)
}

pub fn mime_from_data_url(data_url: &str) -> Option<String> {
    let rest = data_url.strip_prefix("data:")?;
    let (mime, _) = rest.split_once(';')?;
    Some(mime.to_string())
}

pub fn detect_mime(bytes: &[u8], hint: Option<&str>) -> String {
    if let Some(mime) = sniff(bytes) {
        return mime.to_string();
    }
    hint.and_then(|path| {
        std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
    })
    .map(|ext| mime_for_extension(&ext.to_ascii_lowercase()))
    .unwrap_or_else(|| "image/png".to_string())
}

fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Some("image/tiff");
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/ico");
    }
    let head = std::str::from_utf8(&bytes[..bytes.len().min(1024)]).ok()?;
    let head = head.trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return Some("image/svg+xml");
    }
    None
}

fn mime_for_extension(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/ico",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
    .to_string()
}

pub fn decode_render_image(bytes: &[u8], mime: &str) -> Option<Arc<RenderImage>> {
    let rasterized: Option<Vec<u8>> = if mime == "image/svg+xml" {
        crate::render::export::rasterize_svg(bytes, 1.0).ok()
    } else {
        None
    };
    let data = rasterized.as_deref().unwrap_or(bytes);
    let decoded = image::load_from_memory(data).ok()?;
    let mut rgba = decoded.to_rgba8();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let (width, height) = rgba.dimensions();
    let buffer: image::RgbaImage = image::ImageBuffer::from_raw(width, height, rgba.into_raw())?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_const(
        [image::Frame::new(buffer)],
    ))))
}

pub fn render_image_size(render: &RenderImage) -> (f64, f64) {
    let size = render.size(0);
    (size.width.0 as f64, size.height.0 as f64)
}

pub const MAX_INSERT_EDGE: f64 = 1024.0;

pub fn fit_size(width: f64, height: f64) -> (f64, f64) {
    if width <= 0.0 || height <= 0.0 {
        return (200.0, 150.0);
    }
    let longest = width.max(height);
    if longest <= MAX_INSERT_EDGE {
        return (width, height);
    }
    let factor = MAX_INSERT_EDGE / longest;
    (width * factor, height * factor)
}
