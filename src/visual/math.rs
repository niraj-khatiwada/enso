use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

use gpui_kit::base::text::{
    InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
    MarkdownPlugin, markdown_ast,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Hsla, ImageSource, IntoElement, ParentElement as _, Pixels, RenderImage, Rgba,
    Styled as _, Window, div, img, px,
};
use smallvec::SmallVec;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

const MOD: &str = include_str!("../../assets/math/mitex/mod.typ");
const PRELUDE: &str = include_str!("../../assets/math/mitex/prelude.typ");
const STANDARD: &str = include_str!("../../assets/math/mitex/latex/standard.typ");
const CACHE_LIMIT: usize = 512;

struct Assets {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    files: Vec<(FileId, Source)>,
}

static ASSETS: LazyLock<Assets> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    let book = FontBook::from_fonts(&fonts);
    let files = [
        ("/mitex/mod.typ", MOD),
        ("/mitex/prelude.typ", PRELUDE),
        ("/mitex/latex/standard.typ", STANDARD),
    ]
    .into_iter()
    .filter_map(|(path, text)| {
        let id = file_id(path)?;
        Some((id, Source::new(id, text.to_string())))
    })
    .collect();
    Assets {
        library: LazyHash::new(Library::builder().build()),
        book: LazyHash::new(book),
        fonts,
        files,
    }
});

fn file_id(path: &str) -> Option<FileId> {
    let path = VirtualPath::new(path).ok()?;
    Some(RootedPath::new(VirtualRoot::Project, path).intern())
}

struct MathWorld {
    main: Source,
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &ASSETS.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &ASSETS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        ASSETS
            .files
            .iter()
            .find(|(file, _)| *file == id)
            .map(|(_, source)| source.clone())
            .ok_or_else(|| FileError::NotFound(Default::default()))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.source(id)
            .map(|source| Bytes::from_string(source.text().to_string()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        ASSETS.fonts.get(index).cloned()
    }

    fn today(&self, _: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MathKey {
    pub tex: String,
    pub display: bool,
    pub size: u32,
    pub color: u32,
    pub scale: u32,
}

impl MathKey {
    pub fn new(tex: &str, display: bool, size: Pixels, color: Hsla, scale: f32) -> Self {
        let rgba: Rgba = color.into();
        let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
        Self {
            tex: tex.trim().to_string(),
            display,
            size: (f32::from(size) * 4.).round() as u32,
            color: channel(rgba.r) << 24
                | channel(rgba.g) << 16
                | channel(rgba.b) << 8
                | channel(rgba.a),
            scale: (scale * 4.).round().max(4.) as u32,
        }
    }

    fn size_pt(&self) -> f32 {
        self.size as f32 / 4.
    }

    fn scale(&self) -> f32 {
        self.scale as f32 / 4.
    }
}

#[derive(Clone)]
pub struct MathImage {
    pub image: Arc<RenderImage>,
    pub width: Pixels,
    pub height: Pixels,
    pub baseline: Pixels,
}

pub type Rendered = Result<MathImage, String>;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn render(key: &MathKey) -> Rendered {
    let converted = mitex::convert_math(&key.tex, None).map_err(|e| e.to_string())?;
    let color = key.color;
    let main = format!(
        "#import \"/mitex/mod.typ\": mitex-scope\n\
         #set page(width: auto, height: auto, margin: (x: 1pt, y: {margin}pt), fill: none)\n\
         #set text(size: {size}pt, fill: rgb({r}, {g}, {b}, {a}), top-edge: \"bounds\", bottom-edge: \"bounds\")\n\
         #math.equation(block: {display}, eval(\"$\" + \"{tex}\" + \"$\", scope: mitex-scope))\n",
        margin = if key.display { 2 } else { 1 },
        size = key.size_pt(),
        r = color >> 24,
        g = (color >> 16) & 0xff,
        b = (color >> 8) & 0xff,
        a = color & 0xff,
        display = key.display,
        tex = escape(&converted),
    );
    let id = file_id("/main.typ").ok_or("Could not create the formula")?;
    let world = MathWorld {
        main: Source::new(id, main),
    };
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            errors
                .first()
                .map(|e| e.message.to_string())
                .unwrap_or_else(|| "Could not typeset this formula".into())
        })?;
    let page = document.pages().first().ok_or("Empty formula")?;
    let scale = key.scale();
    let pixmap = typst_render::render(
        page,
        &typst_render::RenderOptions {
            pixel_per_pt: typst::utils::Scalar::new(scale as f64),
            render_bleed: false,
        },
    );
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut data = pixmap.take();
    for pixel in data.chunks_exact_mut(4) {
        let a = pixel[3];
        if a > 0 && a < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            }
        }
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(w, h, data).ok_or("Could not draw the formula")?;
    let frame = image::Frame::new(buffer);
    let mut runs = Vec::new();
    text_runs(&page.frame, 0., &mut runs);
    let baseline = runs
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1)))
        .map_or(page.frame.size().y.to_pt(), |(_, y)| *y) as f32;
    Ok(MathImage {
        image: Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1))),
        width: px(w as f32 / scale),
        height: px(h as f32 / scale),
        baseline: px(baseline),
    })
}

fn text_runs(frame: &typst::layout::Frame, offset: f64, out: &mut Vec<(f64, f64)>) {
    for (pos, item) in frame.items() {
        match item {
            typst::layout::FrameItem::Group(group) => {
                text_runs(&group.frame, offset + pos.y.to_pt(), out)
            }
            typst::layout::FrameItem::Text(text) => {
                out.push((text.size.to_pt(), offset + pos.y.to_pt()))
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Cache {
    done: HashMap<MathKey, Rendered>,
    order: Vec<MathKey>,
    pending: HashSet<MathKey>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

pub fn request(key: MathKey, cx: &mut App) -> Option<Rendered> {
    {
        let mut cache = CACHE.lock().ok()?;
        if let Some(done) = cache.done.get(&key) {
            return Some(done.clone());
        }
        if !cache.pending.insert(key.clone()) {
            return None;
        }
    }
    let task = cx.background_executor().spawn({
        let key = key.clone();
        async move { render(&key) }
    });
    cx.spawn(async move |cx| {
        let rendered = task.await;
        if let Ok(mut cache) = CACHE.lock() {
            cache.pending.remove(&key);
            cache.order.push(key.clone());
            cache.done.insert(key, rendered);
            if cache.order.len() > CACHE_LIMIT {
                let old = cache.order.remove(0);
                cache.done.remove(&old);
            }
        }
        cx.update(|cx| cx.refresh_windows());
    })
    .detach();
    None
}

pub fn warm(cx: &App) {
    cx.background_executor()
        .spawn(async {
            LazyLock::force(&ASSETS);
        })
        .detach();
}

pub fn looks_like_math(source: &str) -> bool {
    let Some(inner) = source.strip_prefix('$').and_then(|s| s.strip_suffix('$')) else {
        return false;
    };
    let inner = inner.trim_matches('$');
    !inner.is_empty()
        && !inner.starts_with(char::is_whitespace)
        && !inner.ends_with(char::is_whitespace)
        && !inner
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

pub struct InlineMath {
    pub tex: String,
}

pub struct MathPlugin;

impl MarkdownPlugin for MathPlugin {
    fn name(&self) -> &str {
        "math"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::InlineMath(math) = node else {
            return None;
        };
        let source = cx.node_source(node)?;
        if !looks_like_math(source) {
            return None;
        }
        Some(
            MarkdownNode::new(
                "math",
                InlineMath {
                    tex: math.value.clone(),
                },
            )
            .text(source.to_string())
            .markdown(source.to_string()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        node.as_text().to_string()
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let tex = node.data::<InlineMath>()?.tex.clone();
        let style = context.text_style();
        let key = MathKey::new(
            &tex,
            false,
            context.font_size(),
            style.color,
            window.scale_factor(),
        );
        Some(match request(key, cx) {
            Some(Ok(math)) => InlineElement::new(
                img(ImageSource::Render(math.image))
                    .w(math.width)
                    .h(math.height),
            )
            .with_baseline(math.baseline),
            Some(Err(_)) => InlineElement::new(
                div()
                    .text_color(cx.theme().danger)
                    .child(node.as_text().to_string()),
            ),
            None => InlineElement::new(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(node.as_text().to_string()),
            ),
        })
    }
}

static EXTENSIONS: LazyLock<Arc<MarkdownExtensions>> =
    LazyLock::new(|| Arc::new(MarkdownExtensions::default().plugin(MathPlugin)));
static FRONT_MATTER_EXTENSIONS: LazyLock<Arc<MarkdownExtensions>> = LazyLock::new(|| {
    Arc::new(
        MarkdownExtensions::default()
            .frontmatter()
            .plugin(MathPlugin),
    )
});

pub fn extensions(front_matter: bool) -> Arc<MarkdownExtensions> {
    if front_matter {
        FRONT_MATTER_EXTENSIONS.clone()
    } else {
        EXTENSIONS.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typesets_latex_into_an_image() {
        let key = MathKey {
            tex: r"\nabla_\theta \frac{1}{B} \sum_{i=1}^{B} L_i = \sqrt{x^2 + y^2}".into(),
            display: true,
            size: 17 * 4,
            color: 0x202020ff,
            scale: 8,
        };
        let math = render(&key).unwrap();
        assert!(f32::from(math.width) > 100., "{:?}", math.width);
        assert!(f32::from(math.height) > 20.);
        let inline = render(&MathKey {
            display: false,
            tex: r"\alpha + \beta^2".into(),
            ..key.clone()
        })
        .unwrap();
        assert!(inline.baseline > px(0.) && inline.baseline <= inline.height);
        let plain = render(&MathKey {
            display: false,
            tex: "x".into(),
            ..key.clone()
        })
        .unwrap();
        let ratio = f32::from(plain.baseline) / f32::from(plain.height);
        assert!(
            (0.55..0.95).contains(&ratio),
            "baseline at {ratio} of the height"
        );
        if let Ok(path) = std::env::var("ENSO_MATH_SAMPLE") {
            let sample = render(&MathKey {
                tex: r"\nabla_\theta \frac{1}{B} \sum_{i=1}^{B} L_i = \frac{1}{B} \sum_r \sum_{i \in r} \nabla_\theta L_i, \quad \int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}, \quad \begin{pmatrix} a & b \\ c & d \end{pmatrix}".into(),
                ..key.clone()
            })
            .unwrap();
            let bytes = sample.image.as_bytes(0).unwrap().to_vec();
            let size = sample.image.size(0);
            let mut rgba = bytes;
            for p in rgba.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            image::RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, rgba)
                .unwrap()
                .save_with_format(&path, image::ImageFormat::Png)
                .ok();
        }
        assert!(
            render(&MathKey {
                tex: r"\begin{unknownenv} x".into(),
                ..key
            })
            .is_err()
        );
    }

    #[test]
    fn dollar_amounts_are_not_math() {
        assert!(looks_like_math("$x^2$"));
        assert!(looks_like_math("$\\alpha$"));
        assert!(!looks_like_math("$5 and $"));
        assert!(!looks_like_math("$ x $"));
        assert!(!looks_like_math("$10$"));
    }
}
