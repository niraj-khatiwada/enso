use gpui_excalidraw::{
    core::{
        element::{Element, ElementBase, ElementType},
        geometry::Point,
        scene::Scene,
    },
    render::{
        export::SvgExporter,
        shape::{free_draw_outline, render_element, rough_rectangle},
    },
};

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = ElementBase::new(id.to_string(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    base.seed = 42;
    Element::Rectangle { base }
}

#[test]
fn test_rough_rectangle_generates_closed_path() {
    let opts = Default::default();
    let d = rough_rectangle(0.0, 0.0, 100.0, 50.0, &opts);

    let fill = d.fill_path.as_ref().expect("a fill path");
    assert_eq!(fill.len(), 5);
    assert_eq!(
        fill.last(),
        Some(&gpui_excalidraw::render::shape::Op::Close)
    );

    let outline = &d.sets[0];
    assert!(
        outline
            .iter()
            .any(|op| matches!(op, gpui_excalidraw::render::shape::Op::CubicTo(..))),
        "hand-drawn outline must use cubic curves"
    );
    let moves = outline
        .iter()
        .filter(|op| matches!(op, gpui_excalidraw::render::shape::Op::MoveTo(..)))
        .count();
    assert!(moves >= 2, "expected two overlay passes, got {moves}");
}

#[test]
fn test_render_rectangle_element() {
    let e = rect("a", 0.0, 0.0, 100.0, 100.0);
    let drawables = render_element(&e);
    assert_eq!(drawables.len(), 1);
    assert_eq!(
        drawables[0].shape,
        gpui_excalidraw::render::shape::ShapeKind::Rectangle
    );
}

#[test]
fn test_render_line_element() {
    let mut base = ElementBase::new("l".to_string(), ElementType::Line);
    base.width = 100.0;
    base.height = 0.0;
    let line = Element::Line(gpui_excalidraw::core::element::LineElement {
        linear: gpui_excalidraw::core::element::LinearElement {
            base,
            points: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            start_binding: None,
            end_binding: None,
            start_arrowhead: None,
            end_arrowhead: None,
        },
        polygon: false,
    });
    let drawables = render_element(&line);
    assert_eq!(drawables.len(), 1);
}

#[test]
fn test_svg_export_contains_elements() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    let svg = SvgExporter::export(&scene, "#ffffff");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<path"));
    assert!(svg.ends_with("</svg>"));
}

#[test]
fn test_svg_export_transparent_background() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    let svg = SvgExporter::export(&scene, "transparent");
    assert!(!svg.contains("fill=\"transparent\""));
}

#[test]
fn test_render_text_element() {
    let text = gpui_excalidraw::core::factory::new_text(
        0.0,
        0.0,
        "Hello",
        20.0,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let drawables = render_element(&text);
    assert_eq!(drawables.len(), 1);
    assert_eq!(
        drawables[0].shape,
        gpui_excalidraw::render::shape::ShapeKind::Text
    );
    let t = drawables[0].text.as_ref().unwrap();
    assert_eq!(t.text, "Hello");
    assert_eq!(t.font_size, 20.0);
}

#[test]
fn test_render_image_element() {
    let img = gpui_excalidraw::core::factory::new_image(
        0.0,
        0.0,
        100.0,
        80.0,
        None,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let drawables = render_element(&img);
    assert_eq!(drawables.len(), 1);
    assert_eq!(
        drawables[0].shape,
        gpui_excalidraw::render::shape::ShapeKind::Image
    );
}

#[test]
fn test_render_arrow_with_arrowhead() {
    let arrow = gpui_excalidraw::core::factory::new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        None,
        Some(gpui_excalidraw::core::Arrowhead::Arrow),
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let drawables = render_element(&arrow);
    assert!(drawables.len() >= 2);
    assert!(
        drawables
            .iter()
            .any(|d| d.shape == gpui_excalidraw::render::shape::ShapeKind::Arrowhead)
    );
}

#[test]
fn test_render_arrow_without_arrowhead() {
    let arrow = gpui_excalidraw::core::factory::new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        None,
        None,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let drawables = render_element(&arrow);
    assert_eq!(drawables.len(), 1);
    assert!(
        !drawables
            .iter()
            .any(|d| d.shape == gpui_excalidraw::render::shape::ShapeKind::Arrowhead)
    );
}

#[test]
fn test_dashed_line_splits_segments() {
    let opts = gpui_excalidraw::render::shape::DrawOptions {
        stroke_style: gpui_excalidraw::core::StrokeStyle::Dashed,
        ..Default::default()
    };
    let line = gpui_excalidraw::render::shape::rough_line(
        &[Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        &opts,
    );
    let solid = gpui_excalidraw::render::shape::rough_line(
        &[Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        &Default::default(),
    );
    assert!(line.ops.len() > solid.ops.len());
}

#[test]
fn test_rounded_rectangle_generates_curves() {
    let opts = gpui_excalidraw::render::shape::DrawOptions {
        roundness: Some(gpui_excalidraw::core::types::Roundness {
            kind: gpui_excalidraw::core::types::RoundnessType::ProportionalRadius,
            value: Some(0.25),
        }),
        ..Default::default()
    };
    let d = rough_rectangle(0.0, 0.0, 100.0, 50.0, &opts);

    let has_cubic = d
        .ops
        .iter()
        .any(|op| matches!(op, gpui_excalidraw::render::shape::Op::CubicTo(..)));
    assert!(has_cubic, "rounded rectangle should contain cubic curves");
}

#[test]
fn test_solid_fill_has_fill_path() {
    let mut base = ElementBase::new("f".to_string(), ElementType::Rectangle);
    base.x = 0.0;
    base.y = 0.0;
    base.width = 100.0;
    base.height = 50.0;
    base.fill_style = gpui_excalidraw::core::FillStyle::Solid;
    base.background_color = "#ff0000".to_string();
    let e = Element::Rectangle { base };
    let drawables = render_element(&e);
    assert_eq!(drawables.len(), 1);
    assert!(drawables[0].fill_path.is_some());
}

#[test]
fn test_draw_options_carries_opacity() {
    let mut base = ElementBase::new("o".to_string(), ElementType::Rectangle);
    base.opacity = 50.0;
    base.roundness = Some(gpui_excalidraw::core::types::Roundness {
        kind: gpui_excalidraw::core::types::RoundnessType::AdaptiveRadius,
        value: Some(0.3),
    });
    let e = Element::Rectangle { base };
    let opts = gpui_excalidraw::render::shape::DrawOptions::from_element(&e);
    assert_eq!(opts.opacity, 50.0);
    assert!(opts.roundness.is_some());
}

use gpui_excalidraw::{
    core::{
        Arrowhead,
        factory::{ElementOptions, new_diamond, new_ellipse, new_line, new_rectangle, new_text},
        types::{FillStyle, Roundness, RoundnessType, TextAlign, VerticalAlign},
    },
    render::shape::{Op, ops_to_polygon},
};

fn filled(style: FillStyle) -> ElementOptions {
    ElementOptions {
        background_color: "#a5d8ff".to_string(),
        fill_style: style,
        ..Default::default()
    }
}

fn assert_fill_inside(element: &Element) {
    let d = render_element(element)
        .into_iter()
        .next()
        .expect("a drawable");
    let poly = ops_to_polygon(&d.fill_path.clone().expect("fill path"));

    let min_x = poly.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = poly.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = poly.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = poly.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let tol = 2.0;

    for set in d.sets.iter().skip(1) {
        for op in set {
            let (x, y) = match *op {
                Op::MoveTo(x, y) | Op::LineTo(x, y) => (x, y),
                Op::QuadraticTo(_, _, x, y) => (x, y),
                Op::CubicTo(_, _, _, _, x, y) => (x, y),
                Op::Close => continue,
            };
            assert!(
                x >= min_x - tol && x <= max_x + tol && y >= min_y - tol && y <= max_y + tol,
                "fill point ({x}, {y}) escaped the shape bounds \
                 x[{min_x}, {max_x}] y[{min_y}, {max_y}]"
            );
        }
    }
}

#[test]
fn hachure_fill_stays_inside_rectangle() {
    let e = new_rectangle(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Hachure));
    assert_fill_inside(&e);
}

#[test]
fn hachure_fill_stays_inside_diamond() {
    let e = new_diamond(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Hachure));
    let d = render_element(&e).into_iter().next().unwrap();

    let corner_covered = d.sets.iter().skip(1).flatten().any(|op| match *op {
        Op::MoveTo(x, y) | Op::LineTo(x, y) => x < 5.0 && y < 5.0,
        _ => false,
    });
    assert!(
        !corner_covered,
        "hachure must be clipped away from the corners"
    );
    assert_fill_inside(&e);
}

#[test]
fn hachure_fill_stays_inside_ellipse_and_rounded_rect() {
    let e = new_ellipse(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Hachure));
    assert_fill_inside(&e);

    let mut rounded = new_rectangle(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Hachure));
    if let Element::Rectangle { base } = &mut rounded {
        base.roundness = Some(Roundness {
            kind: RoundnessType::ProportionalRadius,
            value: Some(0.3),
        });
    }
    assert_fill_inside(&rounded);
}

#[test]
fn cross_hatch_produces_two_directions() {
    let e = new_rectangle(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::CrossHatch));
    let d = render_element(&e).into_iter().next().unwrap();

    let plain = new_rectangle(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Hachure));
    let dp = render_element(&plain).into_iter().next().unwrap();
    assert!(d.sets.len() > dp.sets.len());
}

#[test]
fn solid_fill_emits_no_hachure_sets() {
    let e = new_rectangle(0.0, 0.0, 120.0, 80.0, &filled(FillStyle::Solid));
    let d = render_element(&e).into_iter().next().unwrap();
    assert_eq!(d.sets.len(), 1, "solid fill uses the fill_path only");
    assert!(d.fill_path.is_some());
}

#[test]
fn line_renders_arrowheads() {
    let mut line = new_line(
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        &ElementOptions::default(),
    );
    if let Element::Line(l) = &mut line {
        l.linear.end_arrowhead = Some(Arrowhead::Arrow);
        l.linear.start_arrowhead = Some(Arrowhead::Dot);
    }
    let drawables = render_element(&line);

    assert_eq!(drawables.len(), 3);
}

#[test]
fn line_without_arrowheads_renders_one_drawable() {
    let line = new_line(
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        &ElementOptions::default(),
    );
    assert_eq!(render_element(&line).len(), 1);
}

fn centered_text(x: f64, w: f64, align: TextAlign, body: &str) -> Element {
    let mut t = new_text(x, 50.0, body, 20.0, &ElementOptions::default());
    if let Element::Text(te) = &mut t {
        te.base.width = w;
        te.base.height = 40.0;
        te.text_align = align;
        te.vertical_align = VerticalAlign::Middle;
    }
    t
}

#[test]
fn centered_text_anchors_to_box_center_not_left_edge() {
    let el = centered_text(100.0, 200.0, TextAlign::Center, "Hi");
    let svg = SvgExporter::export_element(&el, "#ffffff");

    assert!(svg.contains(r#"text-anchor="middle""#), "svg: {svg}");
    assert!(
        svg.contains(r#"x="200.00""#),
        "expected center x=200, svg: {svg}"
    );
}

#[test]
fn right_aligned_text_anchors_to_box_right_edge() {
    let el = centered_text(100.0, 200.0, TextAlign::Right, "Hi");
    let svg = SvgExporter::export_element(&el, "#ffffff");
    assert!(svg.contains(r#"text-anchor="end""#), "svg: {svg}");
    assert!(
        svg.contains(r#"x="300.00""#),
        "expected right x=300, svg: {svg}"
    );
}

#[test]
fn left_aligned_text_anchors_to_box_left_edge() {
    let el = centered_text(100.0, 200.0, TextAlign::Left, "Hi");
    let svg = SvgExporter::export_element(&el, "#ffffff");
    assert!(svg.contains(r#"text-anchor="start""#), "svg: {svg}");
    assert!(
        svg.contains(r#"x="100.00""#),
        "expected left x=100, svg: {svg}"
    );
}

#[test]
fn multiline_text_emits_one_tspan_per_line() {
    let el = centered_text(0.0, 120.0, TextAlign::Center, "one\ntwo\nthree");
    let svg = SvgExporter::export_element(&el, "#ffffff");
    assert_eq!(svg.matches("<tspan").count(), 3, "svg: {svg}");
    assert!(svg.contains("</text>"), "svg: {svg}");
}

use gpui_excalidraw::core::factory;

fn straight_stroke(step: f64, n: usize) -> Element {
    let points = (0..n).map(|i| Point::new(i as f64 * step, 0.0)).collect();
    factory::new_freedraw(points, &ElementOptions::default())
}

#[test]
fn freedraw_renders_as_filled_ink_rather_than_a_stroked_line() {
    let el = straight_stroke(4.0, 24);
    let ds = render_element(&el);
    assert_eq!(ds.len(), 1);
    let d = &ds[0];
    assert!(d.sets.is_empty(), "ink must not be stroked via sets");
    let ink = d.ink.as_ref().expect("freedraw must carry its outline");
    assert!(
        matches!(ink.first(), Some(Op::MoveTo(..))),
        "outline starts with MoveTo"
    );
    assert!(matches!(ink.last(), Some(Op::Close)), "outline is closed");

    assert!(ink.len() > 2 * 24, "outline len = {}", ink.len());

    let svg = SvgExporter::export_element(&el, "#ffffff");
    assert!(svg.contains(r#"stroke="none""#), "svg: {svg}");
    assert!(svg.contains(r#"fill-rule="nonzero""#), "svg: {svg}");
}

#[test]
fn the_preview_outline_matches_the_committed_ink() {
    let el = straight_stroke(3.0, 18);
    let d = &render_element(&el)[0];
    let committed = d.ink.as_ref().unwrap();

    let base = el.base();
    let pts: Vec<Point> = match &el {
        Element::Freedraw(f) => f
            .points
            .iter()
            .map(|p| Point::new(base.x + p.x, base.y + p.y))
            .collect(),
        _ => unreachable!(),
    };
    let preview = free_draw_outline(&pts, base.stroke_width, true);
    assert_eq!(preview.len(), committed.len() - 1);
}

#[test]
fn faster_strokes_render_thinner_than_slow_ones() {
    let thickness = |step: f64| -> f64 {
        let el = straight_stroke(step, 24);
        let d = &render_element(&el)[0];
        let ink = d.ink.as_ref().unwrap();
        let pts: Vec<(f64, f64)> = ink
            .iter()
            .filter_map(|op| match op {
                Op::MoveTo(x, y) | Op::LineTo(x, y) => Some((*x, *y)),
                _ => None,
            })
            .collect();
        let mut area = 0.0;
        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            area += pts[i].0 * pts[j].1 - pts[j].0 * pts[i].1;
        }
        (area.abs() / 2.0) / (23.0 * step)
    };
    let slow = thickness(1.0);
    let fast = thickness(10.0);
    assert!(
        slow > fast * 1.2,
        "slow stroke ({slow:.2}px mean) should be clearly thicker than fast ({fast:.2}px)"
    );
}

#[test]
fn png_export_produces_a_real_png() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 120.0, 80.0));
    let png = gpui_excalidraw::render::export::export_png(&scene, "#ffffff", 1.0).expect("png");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
}

#[test]
fn png_export_honours_scale() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    let one = gpui_excalidraw::render::export::rasterize_png(
        &SvgExporter::export(&scene, "#ffffff"),
        1.0,
    )
    .unwrap();
    let two = gpui_excalidraw::render::export::rasterize_png(
        &SvgExporter::export(&scene, "#ffffff"),
        2.0,
    )
    .unwrap();
    let dims = |png: &[u8]| {
        (
            u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
            u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
        )
    };
    let (w1, h1) = dims(&one);
    let (w2, h2) = dims(&two);
    assert_eq!((w2, h2), (w1 * 2, h1 * 2));
}

#[test]
fn png_export_background_is_painted() {
    let mut scene = Scene::new();
    scene.add(rect("a", 10.0, 10.0, 40.0, 40.0));
    let png = gpui_excalidraw::render::export::export_png(&scene, "#ff0000", 1.0).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("decode");
    assert!(pixmap.width() >= 40 && pixmap.height() >= 40);
    let pixel = pixmap.pixel(0, 0).unwrap();
    assert_eq!(
        (pixel.red(), pixel.green(), pixel.blue()),
        (0xff, 0x00, 0x00)
    );
    assert_eq!(pixel.alpha(), 0xff);
}

#[test]
fn png_export_keeps_transparent_background() {
    let mut scene = Scene::new();
    scene.add(rect("a", 10.0, 10.0, 40.0, 40.0));
    let png = gpui_excalidraw::render::export::export_png(&scene, "transparent", 1.0).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("decode");
    assert_eq!(pixmap.pixel(0, 0).unwrap().alpha(), 0x00);
}

const RED_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=";

use std::collections::HashMap;

use gpui_excalidraw::core::{
    element::{ImageElement, ImageStatus},
    scene::BinaryFileData,
};

fn image_element(file_id: &str) -> Element {
    let mut base = ElementBase::new("img".to_string(), ElementType::Image);
    base.x = 0.0;
    base.y = 0.0;
    base.width = 20.0;
    base.height = 20.0;
    Element::Image(ImageElement {
        base,
        file_id: Some(file_id.to_string()),
        status: ImageStatus::Saved,
        scale: [1.0, 1.0],
        crop: None,
    })
}

fn red_png_files(file_id: &str) -> HashMap<String, BinaryFileData> {
    let mut files = HashMap::new();
    files.insert(
        file_id.to_string(),
        BinaryFileData::new(
            file_id,
            "image/png",
            format!("data:image/png;base64,{RED_PNG}"),
        ),
    );
    files
}

#[test]
fn svg_export_embeds_a_missing_file_as_a_placeholder() {
    let mut scene = Scene::new();
    scene.add(image_element("file-1"));
    let svg = SvgExporter::export(&scene, "#ffffff");
    assert!(!svg.contains("<image"), "svg: {svg}");
}

#[test]
fn svg_export_embeds_the_real_bitmap() {
    let mut scene = Scene::new();
    scene.add(image_element("file-1"));
    let svg = SvgExporter::export_with_files(&scene, "#ffffff", &red_png_files("file-1"));
    assert!(svg.contains("<image"));
    assert!(svg.contains(&format!("data:image/png;base64,{RED_PNG}")));
    assert!(!svg.contains(">image<"));
}

#[test]
fn png_export_paints_the_embedded_bitmap() {
    let mut scene = Scene::new();
    scene.add(image_element("file-1"));
    let png = gpui_excalidraw::render::export::export_png_with_files(
        &scene,
        "#ffffff",
        1.0,
        &red_png_files("file-1"),
    )
    .unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("decode");
    // The export pads by DEFAULT_EXPORT_PADDING, so the 20x20 bitmap at the
    // scene origin lands at (10, 10); (20, 20) is its centre.
    let pixel = pixmap.pixel(20, 20).unwrap();
    assert_eq!(
        (pixel.red(), pixel.green(), pixel.blue()),
        (0xff, 0x00, 0x00)
    );
}

#[test]
fn container_bound_text_wraps_to_the_box_width() {
    let mut el = new_text(0.0, 0.0, &"字".repeat(30), 20.0, &ElementOptions::default());
    if let Element::Text(t) = &mut el {
        t.container_id = Some("note".to_string());
        t.base.width = 124.0;
    }
    let d = &render_element(&el)[0];
    assert_eq!(d.text.as_ref().unwrap().text.split('\n').count(), 5);

    let svg = SvgExporter::export_element(&el, "#ffffff");
    assert_eq!(svg.matches("<tspan").count(), 5, "svg: {svg}");
}

#[test]
fn free_text_is_not_wrapped() {
    let el = new_text(0.0, 0.0, &"字".repeat(30), 20.0, &ElementOptions::default());
    let d = &render_element(&el)[0];
    assert_eq!(d.text.as_ref().unwrap().text.split('\n').count(), 1);
}

#[test]
fn dark_mode_matches_the_upstream_invert_filter() {
    use gpui_excalidraw::render::export::dark_mode_color;

    // `invert(93%) hue-rotate(180deg)` maps white onto #121212, which is the
    // dark canvas colour upstream ships with.
    assert_eq!(dark_mode_color(255, 255, 255), (18, 18, 18));
    // A bright sticky note has to darken; keeping it bright is the bug this guards.
    let (r, g, b) = dark_mode_color(255, 255, 0);
    assert!(
        r < 90 && g < 90 && b < 40,
        "bright yellow should darken to olive, got ({r}, {g}, {b})"
    );
    // Near-black strokes must flip to near-white so they stay readable.
    let (r, g, b) = dark_mode_color(30, 30, 30);
    assert!(r > 200 && g > 200 && b > 200, "got ({r}, {g}, {b})");
}

#[test]
fn dark_mode_svg_darkens_the_background_and_the_elements() {
    let mut base = ElementBase::new("dm".to_string(), ElementType::Rectangle);
    base.x = 0.0;
    base.y = 0.0;
    base.width = 100.0;
    base.height = 60.0;
    base.stroke_color = "#1e1e1e".to_string();
    base.background_color = "#ffff00".to_string();
    base.fill_style = FillStyle::Solid;
    let mut scene = Scene::new();
    scene.add(Element::Rectangle { base });
    let files = gpui_excalidraw::render::export::BinaryFiles::new();

    let light = SvgExporter::export_with_options(&scene, "#ffffff", &files, false);
    let dark = SvgExporter::export_with_options(&scene, "#ffffff", &files, true);

    assert!(light.contains("fill=\"#ffffff\""), "light: {light}");
    assert!(
        dark.contains("fill=\"#121212\""),
        "the dark export keeps a white background: {dark}"
    );
    assert!(
        !dark.contains("255,255,0"),
        "the sticky note kept its bright yellow fill: {dark}"
    );
}

#[test]
fn an_empty_embeddable_draws_its_placeholder_label() {
    use gpui_excalidraw::core::types::{FontFamily, TextAlign};

    let element = factory::new_embeddable(10.0, 20.0, 400.0, 300.0, &ElementOptions::default());
    let drawables = render_element(&element);
    assert_eq!(drawables.len(), 2, "the frame plus its placeholder label");

    let label = drawables[1].text.as_ref().expect("the placeholder label");
    assert_eq!(label.text, "Empty Web-Embed");
    assert_eq!(label.font_family, FontFamily::Helvetica);
    assert_eq!(label.text_align, TextAlign::Center);
    assert!(label.x >= 10.0 && label.x + label.width <= 410.0);
    assert!(label.y > 20.0 && label.y + label.height < 320.0);
}

#[test]
fn an_embeddable_label_shows_its_link() {
    let mut element = factory::new_embeddable(0.0, 0.0, 400.0, 300.0, &ElementOptions::default());
    element.base_mut().link = Some("https://example.com".to_string());

    let drawables = render_element(&element);
    let label = drawables[1].text.as_ref().expect("the placeholder label");
    assert_eq!(label.text, "https://example.com");
}
