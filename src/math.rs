//! Native TeX math, cached as self-contained outline SVG. The reader, HTML
//! exporter and vector PDF painter all use this drawing and its baseline.
use latex_rust::{Color, Dim, MathFont, MathStyle, SvgOptions};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

pub struct Formula {
    pub svg: Arc<str>,
    pub tree: resvg::usvg::Tree,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}
#[derive(Clone)]
pub struct Equation {
    pub source: String,
    light: Result<Arc<Formula>, String>,
    dark: Result<Arc<Formula>, String>,
}
impl Equation {
    pub fn new(source: &str, display: bool) -> Self {
        Self {
            source: source.into(),
            light: render(source, display, false),
            dark: render(source, display, true),
        }
    }
    pub fn drawing(&self, dark: bool) -> Result<&Arc<Formula>, &String> {
        if dark {
            self.dark.as_ref()
        } else {
            self.light.as_ref()
        }
    }
}

pub fn render(source: &str, display: bool, dark: bool) -> Result<Arc<Formula>, String> {
    type Cache = HashMap<(String, bool, bool), Result<Arc<Formula>, String>>;
    static FONT: OnceLock<Result<MathFont, String>> = OnceLock::new();
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    if source.len() > 16 * 1024 {
        return Err("This equation is too large to render.".into());
    }
    let key = (source.to_owned(), display, dark);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(value) = cache.lock().unwrap().get(&key).cloned() {
        return value;
    }
    let result = (|| {
        let font = FONT
            .get_or_init(|| MathFont::stix_two_math().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(Clone::clone)?;
        let ast = latex_rust::parse(source).map_err(|e| e.to_string())?;
        let size = Dim::from_i64(12); // 16 CSS px, scaled to the surrounding text.
        let layout = latex_rust::layout_with_em_size_pt(
            &ast,
            font,
            if display {
                MathStyle::Display
            } else {
                MathStyle::Text
            },
            &size,
        )
        .map_err(|e| e.to_string())?;
        let options = SvgOptions {
            font_size_pt: size,
            display,
            color: if dark {
                Color::rgb(230, 230, 230)
            } else {
                Color::rgb(31, 35, 40)
            },
        };
        let svg = latex_rust::render_svg(&layout, font, &options).map_err(|e| e.to_string())?;
        let svg = svg[svg.find("<svg").unwrap_or(0)..].to_owned();
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
            .map_err(|e| e.to_string())?;
        let width = tree.size().width();
        let height = tree.size().height();
        let baseline = layout.height.to_svg_string().parse::<f32>().unwrap_or(0.) * 16.;
        Ok(Arc::new(Formula {
            svg: svg.into(),
            tree,
            width,
            height,
            baseline,
        }))
    })();
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 128 {
        cache.clear();
    }
    cache.insert(key, result.clone());
    result
}

/// Used by rich-text clipboard export. Ordinary HTML and PDF retain vectors.
pub fn png(formula: &Formula, scale: f32) -> Option<Vec<u8>> {
    let width = (formula.width * scale).ceil().max(1.) as u32;
    let height = (formula.height * scale).ceil().max(1.) as u32;
    if width > 16384 || height > 16384 || u64::from(width) * u64::from(height) > 32_000_000 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &formula.tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().ok()
}

use crate::*;
use gpui_kit::base::text::{InlineElement, InlineRenderContext};
use gpui_kit::component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};

pub struct InlineMath;
impl MarkdownPlugin for InlineMath {
    fn name(&self) -> &str {
        "enso-inline-math"
    }
    fn parse(&self, node: &Node, context: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::InlineMath(math) = node else {
            return None;
        };
        Some(
            MarkdownNode::new(self.name(), Equation::new(&math.value, false))
                .text(math.value.clone())
                .markdown(context.node_source(node)?.to_owned()),
        )
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let equation = node.data::<Equation>()?;
        let formula = equation.drawing(cx.theme().mode.is_dark()).ok()?;
        let scale = f32::from(context.font_size()) / 16.;
        Some(
            InlineElement::new(
                img(crate::rich::diagram_image(&formula.svg))
                    .w(px(formula.width * scale))
                    .h(px(formula.height * scale)),
            )
            .with_baseline(px(formula.baseline * scale)),
        )
    }
}
pub fn block(equation: &Equation, cx: &App) -> AnyElement {
    match equation.drawing(cx.theme().mode.is_dark()) {
        Ok(formula) => {
            let scale = crate::typography::current(cx).size as f32 / 16.;
            div()
                .debug_selector(|| "math-block".into())
                .w_full()
                .my_3()
                .flex()
                .justify_center()
                .child(
                    img(crate::rich::diagram_image(&formula.svg))
                        .w(px(formula.width * scale))
                        .h(px(formula.height * scale))
                        .max_w_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element()
        }
        Err(error) => v_flex()
            .w_full()
            .my_2()
            .gap_1()
            .text_sm()
            .child(
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(equation.source.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Equation: {error}")),
            )
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn equations_produce_portable_paths_and_real_fraction_matrix_layout() {
        for source in [
            r"E=mc^2",
            r"\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
            r"\sum_{n=1}^{\infty}\frac{1}{n^2}",
            r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        ] {
            let f = render(source, true, false).unwrap_or_else(|e| panic!("{source}: {e}"));
            assert!(f.width > 0. && f.height > 0. && f.baseline > 0.);
            assert!(f.svg.contains("<path"));
            assert!(!f.svg.contains("<text"));
            assert!(png(&f, 2.).unwrap().starts_with(b"\x89PNG"));
        }
        let fraction = render(r"\frac{1}{2}", true, false).unwrap();
        assert!(fraction.height > render("x", true, false).unwrap().height * 1.5);
        assert!(render(r"\unknowncommand{x}", true, false).is_err());
        assert!(render(&"x".repeat(17 * 1024), true, false).is_err());
        let f = render(r"\frac{-b\pm\sqrt{b^2-4ac}}{2a}", true, false).unwrap();
        std::fs::write("/tmp/enso-math-formula.png", png(&f, 4.).unwrap()).unwrap();
    }
}
