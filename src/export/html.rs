//! A portable HTML folder (index.html and public assets), and the bare body that Copy as Rich Text puts on the
//! clipboard with its images inline.

use base64::Engine as _;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

/// Where a note's local images go.
pub enum Images {
    /// Inline as data URIs: the clipboard can't point at files. Large
    /// images are scaled down; `used` counts the bytes inlined so far.
    Inline { used: usize },
    /// Copied into `folder`, linked as `link/name`. The folder is made only
    /// once there's an image to put in it.
    Folder { folder: PathBuf, link: String, copied: HashMap<PathBuf, String> },
    /// Left out.
    None,
}

/// The note's body as HTML, parsed the way the reader parses it (GFM, with
/// enso's inline HTML kept).
pub fn body(source: &str, base: Option<&Path>, images: &mut Images) -> String {
    let options = markdown::Options {
        parse: markdown::ParseOptions::gfm(),
        compile: markdown::CompileOptions {
            // Notes use inline HTML: <img width>, <mark color>, <br>, <u>.
            allow_dangerous_html: true,
            ..markdown::CompileOptions::gfm()
        },
    };
    let html = markdown::to_html_with_options(source, &options)
        .unwrap_or_else(|_| format!("<pre>{}</pre>", escape(source)));
    let inline = matches!(images, Images::Inline { .. });
    diagrams(&rewrite_images(&marks(&html), base, images), inline)
}

/// ```mermaid fences as their diagrams: SVG in a page, which stays sharp at
/// any zoom, or a picture for the clipboard, where most apps drop SVG. A
/// diagram with a mistake stays as its source.
fn diagrams(html: &str, picture: bool) -> String {
    const OPEN: &str = "<pre><code class=\"language-mermaid\">";
    const CLOSE: &str = "</code></pre>";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(OPEN) {
        let Some(len) = rest[start + OPEN.len()..].find(CLOSE) else {
            break;
        };
        let end = start + OPEN.len() + len + CLOSE.len();
        out.push_str(&rest[..start]);
        let source = unescape(&rest[start + OPEN.len()..start + OPEN.len() + len]);
        match crate::diagram::render(&source, false) {
            Ok(drawn) if picture => match crate::diagram::png(&drawn, 2.) {
                Some(png) => out.push_str(&format!(
                    "<p><img src=\"data:image/png;base64,{}\" width=\"{}\" alt=\"Diagram\" /></p>",
                    base64::engine::general_purpose::STANDARD.encode(png),
                    drawn.width.round()
                )),
                None => out.push_str(&rest[start..end]),
            },
            Ok(drawn) => out.push_str(&format!("<figure class=\"diagram\">{}</figure>", drawn.svg)),
            Err(_) => out.push_str(&rest[start..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Write a new export folder. Refuse an existing target rather than replacing
/// an unrelated directory; remove only our new folder if an asset write fails.
pub fn export(
    source: &str,
    base: Option<&Path>,
    title: &str,
    target: &Path,
    with_images: bool,
    font: crate::typography::ReadingFont,
) -> io::Result<()> {
    std::fs::create_dir(target)?;
    let result = (|| {
        let public = target.join("public");
        std::fs::create_dir(&public)?;
        let mut images = if with_images {
            Images::Folder { folder: public.join("images"), link: "public/images".into(), copied: HashMap::new() }
        } else { Images::None };
        let assets = font.assets();
        let mut style = String::new();
        if !assets.is_empty() {
            std::fs::create_dir(public.join("fonts"))?;
            for asset in assets {
                std::fs::write(public.join("fonts").join(&asset.file_name), asset.data)?;
                style.push_str(&format!(
                    "@font-face {{ font-family: \"{}\"; src: url(\"fonts/{}\") format(\"truetype\"); font-weight: {}; font-style: {}; }}\n",
                    font.name(), asset.file_name, if asset.bold { 700 } else { 400 }, if asset.italic { "italic" } else { "normal" }));
            }
            std::fs::write(public.join("fonts/OFL.txt"), font.license())?;
        }
        style.push_str(STYLE);
        if font != crate::typography::ReadingFont::Default {
            style.push_str(&format!("\nbody {{ font-family: \"{}\", {}; }}\n", font.name(), match font {
                crate::typography::ReadingFont::Lora => "serif",
                crate::typography::ReadingFont::Mono | crate::typography::ReadingFont::FiraCode => "monospace",
                _ => "sans-serif",
            }));
        }
        std::fs::write(public.join("style.css"), style)?;
        let page = document(&body(source, base, &mut images), title);
        std::fs::write(target.join("index.html"), page)
    })();
    if result.is_err() { let _ = std::fs::remove_dir_all(target); }
    result
}

/// Save dialogs may still supply a .html suffix. The exported folder has none.
pub fn export_folder(target: &Path) -> PathBuf {
    if target.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm")) {
        target.with_extension("")
    } else { target.to_owned() }
}

/// The note's Markdown without its images (`![…](…)` and `<img>`), for the
/// clipboard's plain-text version: apps that paste plain text would show the
/// image tags and their file paths otherwise.
pub fn plain_text(source: &str) -> String {
    let Ok(tree) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return source.to_string();
    };
    let mut cuts = Vec::new();
    collect_images(&tree, &mut cuts);
    cuts.sort_by_key(|r: &std::ops::Range<usize>| r.start);
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for cut in cuts {
        if cut.start >= at && cut.end <= source.len() {
            out.push_str(&source[at..cut.start]);
            at = cut.end;
        }
    }
    out.push_str(&source[at..]);
    // Lines left empty by a removed image collapse with their neighbours.
    let mut text = String::with_capacity(out.len());
    let mut blank = 0;
    for line in out.lines() {
        blank = if line.trim().is_empty() { blank + 1 } else { 0 };
        if blank < 2 {
            text.push_str(line.trim_end());
            text.push('\n');
        }
    }
    text.trim().to_string() + "\n"
}

fn collect_images(node: &markdown::mdast::Node, cuts: &mut Vec<std::ops::Range<usize>>) {
    use markdown::mdast::Node;
    let image = match node {
        Node::Image(_) | Node::ImageReference(_) => true,
        Node::Html(html) => html.value.trim_start().to_ascii_lowercase().starts_with("<img"),
        _ => false,
    };
    if image {
        if let Some(position) = node.position() {
            cuts.push(position.start.offset..position.end.offset);
        }
        return;
    }
    for child in node.children().into_iter().flatten() {
        collect_images(child, cuts);
    }
}

/// A complete HTML page around `body`.
pub fn document(body: &str, title: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"generator\" content=\"enso\">\n<title>{}</title>\n<link rel=\"stylesheet\" href=\"public/style.css\">\n\
         </head>\n<body>\n<article>\n{body}</article>\n</body>\n</html>\n",
        escape(title)
    )
}

/// enso's `<mark color="…">` (its own shorthand) as CSS browsers and other
/// apps understand.
fn marks(html: &str) -> String {
    html.replace("<mark color=\"", "<mark style=\"background-color: ")
        .replace("<mark color='", "<mark style='background-color: ")
}

/// Point local image sources at their copies (or inline them). Web images
/// and images that can't be read stay as they are.
fn rewrite_images(html: &str, base: Option<&Path>, images: &mut Images) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(tag) = rest.find("<img") {
        let (before, after) = rest.split_at(tag);
        out.push_str(before);
        let end = after.find('>').map_or(after.len(), |i| i + 1);
        out.push_str(&rewrite_tag(&after[..end], base, images));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn rewrite_tag(tag: &str, base: Option<&Path>, images: &mut Images) -> String {
    let Some(at) = tag.find("src=") else {
        return tag.to_string();
    };
    let quote = tag[at + 4..].chars().next().unwrap_or('"');
    if quote != '"' && quote != '\'' {
        return tag.to_string();
    }
    let start = at + 5;
    let Some(len) = tag[start..].find(quote) else {
        return tag.to_string();
    };
    let src = unescape(&tag[start..start + len]);
    if matches!(images, Images::None) {
        return String::new();
    }
    let new = match images {
        Images::Inline { used } => match inline_image(&src, base, used) {
            Some(uri) => Some(uri),
            // Over the budget: the image's description in its place.
            None if *used >= INLINE_BUDGET && local_path(&src, base).is_some() => {
                return escape(&attribute(tag, "alt").unwrap_or_default());
            }
            None => None,
        },
        Images::Folder { folder, link, copied } => copy_image(&src, base, folder, link, copied),
        Images::None => None,
    };
    match new {
        Some(src) => format!("{}{}{}", &tag[..start], escape(&src), &tag[start + len..]),
        None => tag.to_string(),
    }
}

/// Copy a local image into the export's folder once, under a name not yet
/// used there, and return its link.
fn copy_image(
    src: &str,
    base: Option<&Path>,
    folder: &Path,
    link: &str,
    copied: &mut HashMap<PathBuf, String>,
) -> Option<String> {
    let path = local_path(src, base).filter(|p| p.is_file())?;
    if let Some(done) = copied.get(&path) {
        return Some(done.clone());
    }
    std::fs::create_dir_all(folder).ok()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let taken: Vec<&String> = copied.values().collect();
    let mut name = format!("{stem}{extension}");
    let mut n = 2;
    while taken.iter().any(|t| t.ends_with(&format!("/{}", encode(&name)))) {
        name = format!("{stem}-{n}{extension}");
        n += 1;
    }
    std::fs::copy(&path, folder.join(&name)).ok()?;
    let href = format!("{link}/{}", encode(&name));
    copied.insert(path, href.clone());
    Some(href)
}

/// A file name as a URL path segment.
fn encode(name: &str) -> String {
    let mut out = String::new();
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Inlined images stop here, per copy: big enough for a note full of
/// screenshots, small enough that Mail, Docs or Slack still take the paste.
const INLINE_BUDGET: usize = 15 * 1024 * 1024;
/// Images wider than this, or bigger than `SCALE_ABOVE` bytes, are scaled down
/// for the clipboard.
const MAX_WIDTH: u32 = 1600;
const SCALE_ABOVE: usize = 512 * 1024;

/// A local image for the clipboard: as is when it's small, else scaled to
/// `MAX_WIDTH` (JPEG, or PNG when it has transparency). SVGs stay as they
/// are; GIFs too, to keep their animation, unless they're large.
fn inline_image(src: &str, base: Option<&Path>, used: &mut usize) -> Option<String> {
    if *used >= INLINE_BUDGET {
        return None;
    }
    let path = local_path(src, base)?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = std::fs::read(&path).ok()?;
    let (mime, bytes) = match extension.as_str() {
        "svg" => ("image/svg+xml", bytes),
        "gif" if bytes.len() <= 4 * SCALE_ABOVE => ("image/gif", bytes),
        _ => scaled(&bytes, &extension)?,
    };
    let uri = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    if *used + uri.len() > INLINE_BUDGET {
        *used = INLINE_BUDGET;
        return None;
    }
    *used += uri.len();
    Some(uri)
}

/// Image bytes ready to inline: the original if it's small and narrow,
/// otherwise scaled down and re-encoded.
fn scaled(bytes: &[u8], extension: &str) -> Option<(&'static str, Vec<u8>)> {
    let original = match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    let image = image::load_from_memory(bytes).ok()?;
    if bytes.len() <= SCALE_ABOVE && image.width() <= MAX_WIDTH {
        return Some((original, bytes.to_vec()));
    }
    let image = if image.width() > MAX_WIDTH {
        image.resize(MAX_WIDTH, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut out = std::io::Cursor::new(Vec::new());
    if image.color().has_alpha() {
        image.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(("image/png", out.into_inner()))
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
            .encode_image(&image.to_rgb8())
            .ok()?;
        Some(("image/jpeg", out.into_inner()))
    }
}

/// An attribute's value in an HTML tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let end = tag[at..].find('"')? + at;
    Some(unescape(&tag[at..end]))
}

/// The file an image source points at: relative to the note's folder, or a
/// `file://` URL. Web, data and other URLs aren't local.
pub(crate) fn local_path(src: &str, base: Option<&Path>) -> Option<std::path::PathBuf> {
    if let Some(path) = src.strip_prefix("file://") {
        return Some(percent_decode(path).into());
    }
    if src.contains("://") || src.starts_with("data:") || src.starts_with('#') {
        return None;
    }
    let path = std::path::PathBuf::from(percent_decode(src));
    Some(if path.is_absolute() { path } else { base?.join(path) })
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// A quiet, readable page that prints well; follows the reader's light or
/// dark preference on screen.
const STYLE: &str = r#"
:root { color-scheme: light dark; --text: #1f2328; --muted: #59636e; --line: #d1d9e0; --soft: #f6f8fa; --link: #0969da; }
@media (prefers-color-scheme: dark) { :root { --text: #e6e6e6; --muted: #9198a1; --line: #3d444d; --soft: #1c1f24; --link: #4493f8; } body { background: #121212; } }
body { margin: 0; color: var(--text); font: 16px/1.65 -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif; }
article { max-width: 760px; margin: 0 auto; padding: 48px 24px 96px; }
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.6em 0 0.6em; }
h1 { font-size: 2em; } h2 { font-size: 1.5em; } h3 { font-size: 1.25em; }
h1:first-child, h2:first-child, h3:first-child { margin-top: 0; }
p, ul, ol, blockquote, pre, table { margin: 0 0 1em; }
a { color: var(--link); }
img { max-width: 100%; height: auto; }
code { font: 0.875em/1.5 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; background: var(--soft); padding: 0.15em 0.35em; border-radius: 4px; }
pre { background: var(--soft); padding: 14px 16px; border-radius: 8px; overflow-x: auto; }
pre code { background: none; padding: 0; }
blockquote { margin-left: 0; padding: 0 1em; color: var(--muted); border-left: 3px solid var(--line); }
table { border-collapse: collapse; display: block; overflow-x: auto; }
th, td { border: 1px solid var(--line); padding: 6px 12px; }
th { background: var(--soft); }
hr { border: 0; border-top: 1px solid var(--line); margin: 2em 0; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.4em; }
li > input[type="checkbox"] { margin-right: 0.5em; }
.diagram { margin: 0 0 1em; padding: 16px; border-radius: 10px; background: #fafbfc; border: 1px solid var(--line); overflow-x: auto; text-align: center; }
.diagram svg { max-width: 100%; height: auto; }
mark { background: #fef08a; color: inherit; padding: 0 0.15em; border-radius: 3px; }
.footnotes { font-size: 0.875em; color: var(--muted); border-top: 1px solid var(--line); margin-top: 3em; }
@media print { article { padding: 0; max-width: none; } a { color: inherit; } pre, blockquote, table, img { break-inside: avoid; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_put_page_images_and_fonts_in_one_portable_folder() {
        let dir = std::env::temp_dir().join(format!("enso-html-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".assets/images")).unwrap();
        image::RgbImage::new(2, 2).save(dir.join(".assets/images/dot.png")).unwrap();
        let source = "# Title\n\n- [x] done\n\n<mark color=\"#ff000059\">red</mark> and <img src=\".assets/images/dot.png\" width=\"50%\" />\n\n![again](.assets/images/dot.png)\n\n![web](https://example.com/a.png)\n";
        let target = dir.join("My Note");
        export(source, Some(&dir), "Tom & Jerry", &target, true, crate::typography::ReadingFont::Poppins).unwrap();
        let html = std::fs::read_to_string(target.join("index.html")).unwrap();
        assert!(html.contains("<title>Tom &amp; Jerry</title>"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<mark style=\"background-color: #ff000059\">red</mark>"));
        // Each image is copied once and linked relative to the page.
        assert_eq!(html.matches("src=\"public/images/dot.png\"").count(), 2, "{html}");
        assert!(target.join("public/images/dot.png").is_file());
        assert!(html.contains("href=\"public/style.css\""));
        let css = std::fs::read_to_string(target.join("public/style.css")).unwrap();
        assert!(css.contains("font-family: \"Poppins\""));
        assert!(css.contains("url(\"fonts/Poppins-Regular.ttf\")"));
        assert!(css.contains("font-style: italic"));
        for asset in crate::typography::ReadingFont::Poppins.assets() {
            assert_eq!(std::fs::read(target.join("public/fonts").join(asset.file_name)).unwrap(), asset.data);
        }
        assert!(target.join("public/fonts/OFL.txt").is_file());
        assert!(!dir.join("My Note.html").exists());
        assert!(html.contains("width=\"50%\""));
        // Web images stay links; nothing is downloaded.
        assert!(html.contains("src=\"https://example.com/a.png\""));
        // Without images, none are linked or copied.
        let none = body(source, Some(&dir), &mut Images::None);
        assert!(!none.contains("<img"), "{none}");
        // The clipboard gets the image inline instead.
        let inline = body(source, Some(&dir), &mut Images::Inline { used: 0 });
        assert!(inline.contains("src=\"data:image/"), "{inline}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn big_images_are_scaled_down_for_the_clipboard() {
        let dir = std::env::temp_dir().join(format!("enso-html-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::from_fn(4000, 300, |x, _| image::Rgb([(x % 256) as u8, 90, 160]))
            .save(dir.join("wide.png"))
            .unwrap();
        let mut images = Images::Inline { used: 0 };
        let html = body("![wide](wide.png)", Some(&dir), &mut images);
        let data = html.split("base64,").nth(1).unwrap().split('"').next().unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
        let scaled = image::load_from_memory(&bytes).unwrap();
        assert_eq!((scaled.width(), scaled.height()), (1600, 120));
        assert!(html.contains("data:image/jpeg"));
        // Past the budget, an image becomes its description.
        let mut full = Images::Inline { used: INLINE_BUDGET };
        assert!(body("![wide](wide.png)", Some(&dir), &mut full).contains("<p>wide</p>"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plain_text_leaves_images_out() {
        let source = "# Trip\n\n<img src=\".assets/images/a.png\" width=\"100%\" />\n\nText with ![inline](b.png) image.\n\n- item\n";
        assert_eq!(plain_text(source), "# Trip\n\nText with  image.\n\n- item\n");
    }

    #[test]
    fn mermaid_fences_become_diagrams() {
        let source = "Before\n\n```mermaid\nflowchart LR\n  A --> B\n```\n\n```mermaid\nflowchart LR\n  A -->\n```\n";
        let page = body(source, None, &mut Images::None);
        assert!(page.contains("<figure class=\"diagram\"><svg"), "{page}");
        // A mistake stays as its source rather than vanishing.
        assert!(page.contains("<code class=\"language-mermaid\">flowchart LR\n  A --&gt;"), "{page}");
        // The clipboard gets a picture.
        let copied = body(source, None, &mut Images::Inline { used: 0 });
        assert!(copied.contains("src=\"data:image/png;base64,"), "{copied}");
    }

    #[test]
    fn notes_without_local_images_still_export_index_and_public_styles() {
        let dir = std::env::temp_dir().join(format!("enso-html-plain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        export("plain *text*", None, "t", &dir.join("n"), true, crate::typography::ReadingFont::Default).unwrap();
        assert!(dir.join("n/index.html").is_file());
        assert!(dir.join("n/public/style.css").is_file());
        assert!(!dir.join("n/public/images").exists());
        assert!(!dir.join("n_files").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_resolve_against_the_note() {
        let base = Path::new("/notes");
        assert_eq!(local_path("a%20b.png", Some(base)), Some("/notes/a b.png".into()));
        assert_eq!(local_path("/abs/x.png", None), Some("/abs/x.png".into()));
        assert_eq!(local_path("https://x/y.png", Some(base)), None);
        assert_eq!(local_path("rel.png", None), None);
    }
}

