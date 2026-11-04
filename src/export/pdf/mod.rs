//! A note as a PDF: laid out by enso (`layout`), set in the system's fonts
//! (`fonts`), and written by krilla.

mod fonts;
mod layout;

use krilla::action::LinkAction;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::geom::{Path as Shape, PathBuilder, Point, Rect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke};
use krilla::surface::Surface;
use krilla::text::TextDirection;
use layout::{Item, Line, Marker, Panel, Rgb};
use std::path::Path;
use std::sync::OnceLock;

/// Standard page sizes: ISO for most of the world, US sizes for North America.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    A4,
    Letter,
    Legal,
    A5,
    A3,
}

impl Paper {
    pub const ALL: [Self; 5] = [Self::A4, Self::Letter, Self::Legal, Self::A5, Self::A3];

    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::Letter => "Letter",
            Self::Legal => "Legal",
            Self::A5 => "A5",
            Self::A3 => "A3",
        }
    }

    /// Width and height in points, upright.
    fn size(self) -> (f32, f32) {
        match self {
            Self::A4 => (595.28, 841.89),
            Self::Letter => (612., 792.),
            Self::Legal => (612., 1008.),
            Self::A5 => (419.53, 595.28),
            Self::A3 => (841.89, 1190.55),
        }
    }

    /// Letter where it's the norm (the US and Canada, going by the system's
    /// region), A4 everywhere else.
    pub fn local() -> Self {
        static LOCAL: OnceLock<Paper> = OnceLock::new();
        *LOCAL.get_or_init(Self::from_region)
    }

    fn from_region() -> Self {
        let region = std::env::var("LC_PAPER")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        #[cfg(target_os = "macos")]
        let region = if region.is_empty() {
            std::process::Command::new("defaults")
                .args(["read", "-g", "AppleLocale"])
                .output()
                .ok()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default()
        } else {
            region
        };
        let country = region.split(['.', '@']).next().unwrap_or_default();
        if country.ends_with("_US") || country.ends_with("_CA") {
            Self::Letter
        } else {
            Self::A4
        }
    }
}

/// How a PDF is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfLayout {
    pub paper: Paper,
    pub landscape: bool,
    pub images: bool,
    pub font: crate::typography::ReadingFont,
}

const MARGIN_X: f32 = 62.;
const MARGIN_TOP: f32 = 64.;
const MARGIN_BOTTOM: f32 = 70.;

/// Lay `source` (a Markdown note) out and write it as a PDF. `base` is the
/// note's folder, for its images.
pub fn pdf(source: &str, base: Option<&Path>, title: &str, settings: PdfLayout) -> Result<Vec<u8>, String> {
    let pages = typeset(source, base, settings)?;
    let (width, height) = page_size(settings);
    let mut document = krilla::Document::new();
    let mut headings: Vec<(u8, String, usize, f32)> = Vec::new();
    let total = pages.len();
    for (index, page_items) in pages.iter().enumerate() {
        let page_settings = PageSettings::from_wh(width, height).ok_or("Invalid page size")?;
        let mut page = document.start_page_with(page_settings);
        let mut links = Vec::new();
        let mut surface = page.surface();
        for (y, item) in page_items {
            let top = MARGIN_TOP + y;
            paint(&mut surface, item, top, &mut links);
            if let Some(Line { heading: Some((level, text)), .. }) = item.line() {
                headings.push((*level, text.clone(), index, top));
            }
        }
        if total > 1 {
            page_number(&mut surface, index + 1, width, height, settings.font);
        }
        surface.finish();
        for (rect, url) in links {
            page.add_annotation(Annotation::new_link(
                LinkAnnotation::new(rect, Target::Action(LinkAction::new(url).into())),
                None,
            ));
        }
        page.finish();
    }
    document.set_outline(outline(&headings));
    document.set_metadata(Metadata::new().title(title.to_string()).creator("enso".to_string()));
    document.finish().map_err(|e| format!("{e:?}"))
}

fn page_size(settings: PdfLayout) -> (f32, f32) {
    let (w, h) = settings.paper.size();
    if settings.landscape { (h, w) } else { (w, h) }
}

/// The note laid out and cut into pages: each item with its offset from the
/// top of the page's content area.
fn typeset(source: &str, base: Option<&Path>, settings: PdfLayout) -> Result<Vec<Vec<(f32, Item)>>, String> {
    let mut options = markdown::ParseOptions::gfm();
    // Match the reader's block and inline math syntax.
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let tree = markdown::to_mdast(source, &options).map_err(|e| e.to_string())?;
    let fonts = fonts::Fonts::get(settings.font)?;
    let (width, height) = page_size(settings);
    let content_height = height - MARGIN_TOP - MARGIN_BOTTOM;
    let mut layout = layout::Layout::new(fonts, base, settings.images, content_height);
    layout.document(&tree, width - 2. * MARGIN_X);
    Ok(paginate(layout.items, content_height))
}

/// Fill pages top to bottom. Space never starts a page, and a heading never
/// ends one: it moves over with the block it introduces.
fn paginate(items: Vec<Item>, height: f32) -> Vec<Vec<(f32, Item)>> {
    let heights: Vec<f32> = items.iter().map(Item::height).collect();
    // Headings, and a table's header row, stay with what follows them.
    let keeps: Vec<bool> = items
        .iter()
        .map(Item::keep_with_next)
        .collect();
    let spaces: Vec<bool> = items.iter().map(Item::spacer).collect();
    let mut pages: Vec<Vec<(f32, Item)>> = vec![Vec::new()];
    let mut y = 0f32;
    for (i, item) in items.into_iter().enumerate() {
        let is_space = matches!(item, Item::Space { panel: None, .. });
        if is_space && y == 0. {
            continue;
        }
        // A heading takes the next real item's height along with it.
        let mut needed = heights[i];
        if keeps[i] {
            let mut j = i + 1;
            while j < heights.len() && (keeps[j] || spaces[j]) {
                needed += heights[j];
                j += 1;
            }
            if let Some(next) = heights.get(j) {
                needed += next;
            }
        }
        if y > 0. && y + needed > height {
            pages.push(Vec::new());
            y = 0.;
            if is_space {
                continue;
            }
        }
        pages.last_mut().unwrap().push((y, item));
        y += heights[i];
    }
    if pages.last().is_some_and(Vec::is_empty) && pages.len() > 1 {
        pages.pop();
    }
    pages
}

fn color(rgb_: Rgb) -> krilla::paint::Paint {
    rgb::Color::new(rgb_.0, rgb_.1, rgb_.2).into()
}

fn fill(surface: &mut Surface, rgb_: Rgb, opacity: f32) {
    surface.set_stroke(None);
    surface.set_fill(Some(Fill {
        paint: color(rgb_),
        opacity: NormalizedF32::new(opacity.clamp(0., 1.)).unwrap_or(NormalizedF32::ONE),
        rule: FillRule::NonZero,
    }));
}

fn stroke(surface: &mut Surface, rgb_: Rgb, width: f32) {
    surface.set_fill(None);
    surface.set_stroke(Some(Stroke { paint: color(rgb_), width, ..Default::default() }));
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Shape> {
    let mut path = PathBuilder::new();
    path.push_rect(Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))?);
    path.finish()
}

/// A rectangle with its top and/or bottom corners rounded.
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32, top: bool, bottom: bool) -> Option<Shape> {
    let r = r.min(w / 2.).min(h / 2.);
    let (rt, rb) = (if top { r } else { 0. }, if bottom { r } else { 0. });
    let k = 0.5523; // Bézier circle approximation
    let mut p = PathBuilder::new();
    p.move_to(x + rt, y);
    p.line_to(x + w - rt, y);
    if rt > 0. {
        p.cubic_to(x + w - rt + rt * k, y, x + w, y + rt - rt * k, x + w, y + rt);
    }
    p.line_to(x + w, y + h - rb);
    if rb > 0. {
        p.cubic_to(x + w, y + h - rb + rb * k, x + w - rb + rb * k, y + h, x + w - rb, y + h);
    }
    p.line_to(x + rb, y + h);
    if rb > 0. {
        p.cubic_to(x + rb - rb * k, y + h, x, y + h - rb + rb * k, x, y + h - rb);
    }
    p.line_to(x, y + rt);
    if rt > 0. {
        p.cubic_to(x, y + rt - rt * k, x + rt - rt * k, y, x + rt, y);
    }
    p.close();
    p.finish()
}

fn draw(surface: &mut Surface, shape: Option<Shape>) {
    if let Some(shape) = shape {
        surface.draw_path(&shape);
    }
}

fn paint_panel(surface: &mut Surface, panel: &Option<Panel>, top: f32, height: f32) {
    if let Some(p) = panel {
        fill(surface, p.color, 1.);
        // A hair of overlap so stacked pieces read as one panel.
        draw(surface, rounded(MARGIN_X + p.x, top - 0.3, p.width, height + 0.6, 6., p.round_top, p.round_bottom));
    }
}

fn paint_bars(surface: &mut Surface, bars: &[f32], top: f32, height: f32) {
    for x in bars {
        fill(surface, layout::QUOTE_BAR, 1.);
        draw(surface, rect(MARGIN_X + x, top - 0.3, 2.5, height + 0.6));
    }
}

fn paint(surface: &mut Surface, item: &Item, top: f32, links: &mut Vec<(Rect, String)>) {
    match item {
        Item::Space { height, bars, panel } => {
            paint_panel(surface, panel, top, *height);
            paint_bars(surface, bars, top, *height);
        }
        Item::Line(line) => paint_line(surface, line, 0., top, links),
        Item::Formula{x,width,height:_,formula}=>paint_formula(surface,formula,MARGIN_X+x,top,*width/formula.width.max(1.)),
        Item::Callout{panel,accent,item}=>{
            let height=item.height();paint_panel(surface,&Some(*panel),top,height);
            fill(surface,*accent,0.65);draw(surface,rect(MARGIN_X+panel.x,top-0.3,2.,height+0.6));
            paint(surface,item,top,links);
        },
        Item::Rule { x, width } => {
            fill(surface, layout::BORDER, 1.);
            draw(surface, rect(MARGIN_X + x, top + 7.5, *width, 0.8));
        }
        Item::Image { x, width, height, image } => {
            surface.push_transform(&krilla::geom::Transform::from_translate(MARGIN_X + x, top));
            if let Some(size) = krilla::geom::Size::from_wh(*width, *height) {
                surface.draw_image(image.clone(), size);
            }
            surface.pop();
        }
        Item::Row(row) => {
            if row.header {
                fill(surface, layout::SOFT, 1.);
                draw(surface, rect(MARGIN_X + row.x, top, row.width, row.height));
            }
            stroke(surface, layout::BORDER, 0.6);
            // Every row draws its top edge, so a table continued on a new
            // page starts closed.
            draw(surface, line_shape(MARGIN_X + row.x, top, MARGIN_X + row.x + row.width, top));
            draw(surface, line_shape(MARGIN_X + row.x, top + row.height, MARGIN_X + row.x + row.width, top + row.height));
            for cell in &row.cells {
                draw(surface, line_shape(MARGIN_X + cell.x, top, MARGIN_X + cell.x, top + row.height));
            }
            draw(surface, line_shape(MARGIN_X + row.x + row.width, top, MARGIN_X + row.x + row.width, top + row.height));
            for cell in &row.cells {
                let mut y = top + 5.;
                for line in &cell.lines {
                    paint_line(surface, line, 0., y, links);
                    y += line.height;
                }
            }
        }
    }
}

fn paint_formula(surface:&mut Surface,formula:&crate::math::Formula,x:f32,y:f32,scale:f32){
    surface.push_transform(&krilla::geom::Transform::from_translate(x,y));
    surface.push_transform(&krilla::geom::Transform::from_scale(scale,scale));
    paint_math_group(surface,formula.tree.root());
    surface.pop();surface.pop();
}
fn paint_math_group(surface:&mut Surface,group:&resvg::usvg::Group){
    use resvg::usvg::{Node,Paint};
    use resvg::tiny_skia::PathSegment;
    for node in group.children(){match node{
        Node::Group(group)=>paint_math_group(surface,group),
        Node::Path(path)=>{
            let Some(paint)=path.fill()else{continue};
            let Paint::Color(c)=paint.paint()else{continue};
            fill(surface,(c.red,c.green,c.blue),paint.opacity().get());
            let mut builder=PathBuilder::new();
            for segment in path.data().segments(){match segment{
                PathSegment::MoveTo(p)=>builder.move_to(p.x,p.y),
                PathSegment::LineTo(p)=>builder.line_to(p.x,p.y),
                PathSegment::QuadTo(a,b)=>builder.quad_to(a.x,a.y,b.x,b.y),
                PathSegment::CubicTo(a,b,c)=>builder.cubic_to(a.x,a.y,b.x,b.y,c.x,c.y),
                PathSegment::Close=>builder.close(),
            }}
            let t=path.abs_transform();
            surface.push_transform(&krilla::geom::Transform::from_row(t.sx,t.ky,t.kx,t.sy,t.tx,t.ty));
            draw(surface,builder.finish());surface.pop();
        },_=>{},
    }}
}

fn circle(cx: f32, cy: f32, r: f32) -> Option<Shape> {
    rounded(cx - r, cy - r, 2. * r, 2. * r, r, true, true)
}

fn line_shape(x1: f32, y1: f32, x2: f32, y2: f32) -> Option<Shape> {
    let mut p = PathBuilder::new();
    p.move_to(x1, y1);
    p.line_to(x2, y2);
    p.finish()
}

fn paint_line(surface: &mut Surface, line: &Line, dx: f32, top: f32, links: &mut Vec<(Rect, String)>) {
    paint_panel(surface, &line.panel, top, line.height);
    paint_bars(surface, &line.bars, top, line.height);
    let baseline = top + line.baseline;
    // Backgrounds first, text over them: one shape per run of touching
    // fragments, so translucent highlights don't darken where words meet.
    let mut i = 0;
    while i < line.frags.len() {
        let first = &line.frags[i];
        let kind = (first.style.highlight, first.style.code);
        let mut j = i + 1;
        while j < line.frags.len()
            && (line.frags[j].style.highlight, line.frags[j].style.code) == kind
            && (line.frags[j].x - (line.frags[j - 1].x + line.frags[j - 1].width)).abs() < 0.5
        {
            j += 1;
        }
        let run = &line.frags[i..j];
        let (start, end) = (run[0].x, run[run.len() - 1].x + run[run.len() - 1].width);
        let x = MARGIN_X + dx + start;
        let above = run.iter().map(|f| f.face.ascent * f.style.size).fold(0., f32::max);
        let below = run.iter().map(|f| f.face.descent * f.style.size).fold(0., f32::max);
        if let Some((rgb_, alpha)) = kind.0 {
            fill(surface, rgb_, alpha);
            draw(surface, rounded(x - 1., baseline - above - 1., end - start + 2., above + below + 2., 2.5, true, true));
        }
        if kind.1 && run.iter().any(|f| !f.text.trim().is_empty()) {
            fill(surface, layout::CODE_INLINE, 1.);
            draw(surface, rounded(x - 2., baseline - above - 1.5, end - start + 4., above + below + 3., 3., true, true));
        }
        i = j;
    }
    for frag in &line.frags {
        let x = MARGIN_X + dx + frag.x;
        let size = frag.style.size;
        let y = baseline - frag.style.raise;
        if let Some(formula)=&frag.math {
            let scale=size/16.;paint_formula(surface,formula,x,y-formula.baseline*scale,scale);
        }
        if !frag.text.trim().is_empty() {
            fill(surface, frag.style.color, 1.);
            surface.draw_text(Point::from_xy(x, y), frag.face.font.clone(), size, &frag.text, false, TextDirection::Auto);
        }
        if frag.style.underline {
            fill(surface, frag.style.color, 1.);
            draw(surface, rect(x, y + size * 0.13, frag.width, (size * 0.06).max(0.5)));
        }
        if frag.style.strike {
            fill(surface, frag.style.color, 1.);
            draw(surface, rect(x, y - size * 0.3, frag.width, (size * 0.06).max(0.5)));
        }
        if let Some(url) = &frag.style.link {
            if let Some(r) = Rect::from_xywh(x, y - frag.face.ascent * size, frag.width, (frag.face.ascent + frag.face.descent) * size) {
                links.push((r, url.clone()));
            }
        }
    }
    match &line.marker {
        Some(Marker::Text { x, text, face, size, color }) => {
            fill(surface, *color, 1.);
            surface.draw_text(Point::from_xy(MARGIN_X + dx + x, baseline), face.font.clone(), *size, text, false, TextDirection::Auto);
        }
        Some(Marker::Bullet { x, size, depth, color: c }) => {
            let r = size * 0.16;
            let (cx, cy) = (MARGIN_X + dx + x + r, baseline - size * 0.32);
            match depth {
                0 => {
                    fill(surface, *c, 1.);
                    draw(surface, circle(cx, cy, r));
                }
                1 => {
                    stroke(surface, *c, 0.9);
                    draw(surface, circle(cx, cy, r - 0.45));
                }
                _ => {
                    fill(surface, *c, 1.);
                    draw(surface, rect(cx - r * 0.85, cy - r * 0.85, r * 1.7, r * 1.7));
                }
            }
        }
        Some(Marker::Check { x, size, done }) => {
            let s = size * 0.95;
            let (bx, by) = (MARGIN_X + dx + x, baseline - s * 0.82);
            if *done {
                fill(surface, layout::TEXT, 1.);
                draw(surface, rounded(bx, by, s, s, 2.5, true, true));
                surface.set_fill(None);
                surface.set_stroke(Some(Stroke {
                    paint: color((255, 255, 255)),
                    width: 1.4,
                    line_cap: krilla::paint::LineCap::Round,
                    line_join: krilla::paint::LineJoin::Round,
                    ..Default::default()
                }));
                let mut p = PathBuilder::new();
                p.move_to(bx + s * 0.24, by + s * 0.52);
                p.line_to(bx + s * 0.43, by + s * 0.7);
                p.line_to(bx + s * 0.77, by + s * 0.3);
                draw(surface, p.finish());
            } else {
                stroke(surface, (0x8c, 0x95, 0x9f), 1.);
                draw(surface, rounded(bx + 0.5, by + 0.5, s - 1., s - 1., 2.5, true, true));
            }
        }
        None => {}
    }
}

fn page_number(surface: &mut Surface, number: usize, width: f32, height: f32, font: crate::typography::ReadingFont) {
    let Ok(fonts) = fonts::Fonts::get(font) else { return };
    let face = fonts.face(fonts::Variant::default());
    let text = number.to_string();
    let size = 8.5;
    let w = face.width(&text, size);
    fill(surface, layout::MUTED, 1.);
    surface.draw_text(
        Point::from_xy((width - w) / 2., height - MARGIN_BOTTOM / 2. + 4.),
        face.font.clone(),
        size,
        &text,
        false,
        TextDirection::Auto,
    );
}

/// Bookmarks from the headings, nested by level.
fn outline(headings: &[(u8, String, usize, f32)]) -> Outline {
    fn build(headings: &[(u8, String, usize, f32)], at: &mut usize, level: u8) -> Vec<OutlineNode> {
        let mut nodes = Vec::new();
        while let Some((l, text, page, y)) = headings.get(*at) {
            if *l < level {
                break;
            }
            *at += 1;
            let mut node = OutlineNode::new(text.clone(), XyzDestination::new(*page, Point::from_xy(0., *y)));
            for child in build(headings, at, l + 1) {
                node.push_child(child);
            }
            nodes.push(node);
        }
        nodes
    }
    let mut outline = Outline::new();
    let mut at = 0;
    let top = headings.iter().map(|h| h.0).min().unwrap_or(1);
    for node in build(headings, &mut at, top) {
        outline.push_child(node);
    }
    outline
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: PdfLayout = PdfLayout { paper: Paper::A4, landscape: false, images: true, font: crate::typography::ReadingFont::Default };

    #[test]
    fn the_features_sample_becomes_a_pdf() {
        let dir = std::env::temp_dir().join(format!("enso-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::from_fn(40, 20, |x, _| image::Rgb([(x * 6) as u8, 120, 200]))
            .save(dir.join("dot.png"))
            .unwrap();
        let source = format!(
            "{SAMPLE}\n\n![dot](dot.png)\n\n<img src=\"dot.png\" width=\"50%\" />\n\n![web](https://example.com/x.png)\n"
        );
        let pdf = pdf(&source, Some(&dir), "Sample", LAYOUT).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        // The diagram is a picture; the broken one stays as code.
        let pages = typeset(&source, Some(&dir), LAYOUT).unwrap();
        let pictures = pages.iter().flatten().filter(|(_, i)| matches!(i, Item::Image { .. })).count();
        assert_eq!(pictures, 3, "two images and one diagram");
        let without = pdf_no_images(&source, &dir);
        assert!(without.len() < pdf.len());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn pdf_no_images(source: &str, dir: &Path) -> Vec<u8> {
        pdf(source, Some(dir), "Sample", PdfLayout { images: false, ..LAYOUT }).unwrap()
    }

    #[test]
    fn long_notes_flow_onto_more_pages_and_headings_stay_with_their_text() {
        let mut source = String::new();
        for n in 0..60 {
            source.push_str(&format!("## Section {n}\n\n{}\n\n", "Words that wrap across the line. ".repeat(12)));
        }
        let pages = typeset(&source, None, LAYOUT).unwrap();
        assert!(pages.len() > 5, "{} pages", pages.len());
        for page in &pages {
            // No page ends on a heading.
            let last = page.iter().rev().find(|(_, i)| !matches!(i, Item::Space { .. }));
            assert!(!matches!(last, Some((_, Item::Line(l))) if l.keep_with_next));
            // Nothing runs past the bottom margin.
            let (w, h) = page_size(LAYOUT);
            let _ = w;
            let bottom = page.last().map(|(y, i)| y + i.height()).unwrap_or(0.);
            assert!(bottom <= h - MARGIN_TOP - MARGIN_BOTTOM + 0.5);
        }
    }

    #[test]
    fn lines_never_overflow_the_page_width() {
        let source = format!(
            "{} https://example.com/{}\n\n| a | b |\n|---|---|\n| {} | x |\n\n```\n{}\n```\n",
            "word ".repeat(80),
            "x".repeat(300),
            "cell ".repeat(60),
            "y".repeat(400)
        );
        let pages = typeset(&source, None, LAYOUT).unwrap();
        let (w, _) = page_size(LAYOUT);
        let limit = w - 2. * MARGIN_X + 0.5;
        for (_, item) in pages.iter().flatten() {
            let lines: Vec<&Line> = match item {
                Item::Line(l) => vec![l],
                Item::Row(r) => r.cells.iter().flat_map(|c| &c.lines).collect(),
                _ => vec![],
            };
            for line in lines {
                for f in &line.frags {
                    assert!(f.x + f.width <= limit, "{:?} ends at {}", f.text, f.x + f.width);
                }
            }
        }
    }

    #[test]
    fn landscape_letter_pages() {
        let settings = PdfLayout { paper: Paper::Letter, landscape: true, ..LAYOUT };
        assert_eq!(page_size(settings), (792., 612.));
        assert!(pdf("x", None, "t", settings).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn selected_bundled_fonts_are_embedded_in_the_pdf() {
        use crate::typography::ReadingFont;
        for (font, name) in [(ReadingFont::Inter, "Inter"), (ReadingFont::Lora, "Lora"),
            (ReadingFont::Mono, "JetBrainsMono"), (ReadingFont::Poppins, "Poppins"),
            (ReadingFont::FiraCode, "FiraCode")] {
            let bytes = pdf("# Heading\n\nRegular **bold** *italic* ***both***.", None, "Fonts", PdfLayout { font, ..LAYOUT }).unwrap();
            let contents = String::from_utf8_lossy(&bytes);
            assert!(contents.contains(name), "selected {name} font must be embedded");
            assert!(contents.contains("/FontFile2") || contents.contains("/FontFile3"), "PDF must contain font data");
        }
    }

    #[test]
    fn math_and_callouts_export_as_vectors_even_with_images_disabled() {
        let source = r"# Math and callouts

> [!NOTE]
> A calm note with **formatted** text and $x^2$.

> [!TIP] A useful identity
> Inline $e^{i\pi}+1=0$ fits within this sentence.

> [!WARNING]
> Keep the original files before replacing a workspace.

$$
\frac{-b\pm\sqrt{b^2-4ac}}{2a}
$$

$$
\begin{pmatrix}a&b\\c&d\end{pmatrix}
$$
";
        let layout = PdfLayout { images:false, ..LAYOUT };
        let pages = typeset(source,None,layout).unwrap();
        assert!(pages.iter().flatten().any(|(_,item)|matches!(item,Item::Formula{..})));
        assert!(pages.iter().flatten().any(|(_,item)|matches!(item,Item::Callout{..})));
        fn has_math(item:&Item)->bool {
            match item {Item::Line(line)=>line.frags.iter().any(|f|f.math.is_some()),Item::Callout{item,..}=>has_math(item),_=>false}
        }
        assert!(pages.iter().flatten().any(|(_,item)|has_math(item)));
        let bytes = pdf(source,None,"Math and callouts",layout).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        std::fs::write("/tmp/enso-callout-math.pdf",bytes).unwrap();
        let long=format!("> [!NOTE] Long note\n{}", "> A paragraph that flows across pages.\n>\n".repeat(160));
        let pages=typeset(&long,None,layout).unwrap();
        assert!(pages.len()>1);
        let height=page_size(layout).1-MARGIN_TOP-MARGIN_BOTTOM;
        for page in pages {for (top,item) in page {assert!(top+item.height()<=height+0.5);}}
    }

    const SAMPLE: &str = r##"# Heading 1
## Heading 2
###### Heading 6

Plain, **bold**, *italic*, ***both***, ~~struck~~, `code`, and a [link](https://commonmark.org "title").
Line one
line two\
<b>B</b> <i>I</i> <s>S</s> <u>U</u> <mark>default</mark> <mark style="background-color: #33996655">green</mark> 中文 日本語 한국어

> Quote with **bold**
>
> > Nested

- One
  - Nested
- Two

3. Three
4. Four

- [x] Done
- [ ] Open
  - [x] Nested done

| Left | Center | Right |
| :--- | :----: | ----: |
| a | **b** | ~~c~~ |

<table><tr><th>H</th></tr><tr><td>c</td></tr></table>

```rust
fn main() { println!("{}", "#[x]"); }
```

```mermaid
flowchart LR
  A[Write] --> B{Export?}
  B -->|PDF| C[Print]
```

```mermaid
flowchart LR
  A -->
```

$$
E = mc^2
$$

A footnote.[^1] Reference [link][gfm].

[^1]: The *note*.
[gfm]: https://github.github.com/gfm/

---

\*escaped\* \# not a heading
"##;
}


