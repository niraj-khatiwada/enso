use std::{collections::HashMap, sync::Arc};

use gpui_kit::*;

use crate::{
    core::{element::Element, scene::Scene},
    render::shape::{Op, TextDrawable, render_element_with_lookup},
};

fn rgba_hex(r: f64, g: f64, b: f64, a: f64) -> Rgba {
    let r = (r.clamp(0.0, 1.0) * 255.0) as u32;
    let g = (g.clamp(0.0, 1.0) * 255.0) as u32;
    let b = (b.clamp(0.0, 1.0) * 255.0) as u32;
    rgb((r << 16) | (g << 8) | b).alpha(a as f32)
}

pub fn paint_scene(
    scene: &Scene,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    files: &HashMap<String, Arc<RenderImage>>,
    window: &mut Window,
    cx: &mut App,
) {
    let elements: Vec<Element> = scene.non_deleted().cloned().collect();
    paint_elements(&elements, zoom, scroll_x, scroll_y, files, window, cx);
}

pub fn paint_elements(
    elements: &[Element],
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    files: &HashMap<String, Arc<RenderImage>>,
    window: &mut Window,
    cx: &mut App,
) {
    let lookup: HashMap<&str, &Element> = elements.iter().map(|e| (e.id(), e)).collect();
    for element in elements {
        for d in render_element_with_lookup(element, &lookup) {
            if let Some(t) = &d.text {
                paint_text(t, zoom, scroll_x, scroll_y, window, cx);
                continue;
            }
            if let Some(img) = &d.image {
                paint_image(img, zoom, scroll_x, scroll_y, files, window);
                continue;
            }

            if let Some(ink) = &d.ink {
                let mut ib = PathBuilder::fill();
                for op in ink {
                    match op {
                        Op::MoveTo(x, y) => {
                            ib.move_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::LineTo(x, y) => {
                            ib.line_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::QuadraticTo(cx0, cy0, x, y) => {
                            ib.curve_to(
                                point(
                                    px((scroll_x + x * zoom) as f32),
                                    px((scroll_y + y * zoom) as f32),
                                ),
                                point(
                                    px((scroll_x + cx0 * zoom) as f32),
                                    px((scroll_y + cy0 * zoom) as f32),
                                ),
                            );
                        }
                        Op::Close => {
                            ib.close();
                        }
                        _ => {}
                    }
                }
                if let Ok(path) = ib.build() {
                    let opacity = (d.options.opacity / 100.0).clamp(0.0, 1.0);
                    let c = d.options.stroke_color;
                    let color = rgba_hex(c.r, c.g, c.b, c.a * opacity);
                    window.paint_path(path, color);
                }
                continue;
            }
            if d.options.fill_style == crate::core::types::FillStyle::Solid
                && let (Some(fill_path), Some(fill_color)) = (&d.fill_path, d.options.fill_color)
            {
                let mut fb = PathBuilder::fill();
                for op in fill_path {
                    match op {
                        Op::MoveTo(x, y) => {
                            fb.move_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::LineTo(x, y) => {
                            fb.line_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::QuadraticTo(cx0, cy0, x, y) => {
                            fb.curve_to(
                                point(
                                    px((scroll_x + x * zoom) as f32),
                                    px((scroll_y + y * zoom) as f32),
                                ),
                                point(
                                    px((scroll_x + cx0 * zoom) as f32),
                                    px((scroll_y + cy0 * zoom) as f32),
                                ),
                            );
                        }
                        Op::Close => {
                            fb.close();
                        }
                        _ => {}
                    }
                }
                if let Ok(path) = fb.build() {
                    let opacity = (d.options.opacity / 100.0).clamp(0.0, 1.0);
                    let color = rgba_hex(
                        fill_color.r,
                        fill_color.g,
                        fill_color.b,
                        fill_color.a * opacity,
                    );
                    window.paint_path(path, color);
                }
            }
            for set in &d.sets {
                let mut builder = PathBuilder::stroke(px((d.options.stroke_width * zoom) as f32));
                for op in set {
                    match op {
                        Op::MoveTo(x, y) => {
                            builder.move_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::LineTo(x, y) => {
                            builder.line_to(point(
                                px((scroll_x + x * zoom) as f32),
                                px((scroll_y + y * zoom) as f32),
                            ));
                        }
                        Op::QuadraticTo(cx0, cy0, x, y) => {
                            builder.curve_to(
                                point(
                                    px((scroll_x + x * zoom) as f32),
                                    px((scroll_y + y * zoom) as f32),
                                ),
                                point(
                                    px((scroll_x + cx0 * zoom) as f32),
                                    px((scroll_y + cy0 * zoom) as f32),
                                ),
                            );
                        }
                        Op::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                            builder.cubic_bezier_to(
                                point(
                                    px((scroll_x + x * zoom) as f32),
                                    px((scroll_y + y * zoom) as f32),
                                ),
                                point(
                                    px((scroll_x + c1x * zoom) as f32),
                                    px((scroll_y + c1y * zoom) as f32),
                                ),
                                point(
                                    px((scroll_x + c2x * zoom) as f32),
                                    px((scroll_y + c2y * zoom) as f32),
                                ),
                            );
                        }
                        Op::Close => {
                            builder.close();
                        }
                    }
                }
                if let Ok(path) = builder.build() {
                    let color = rgba_hex(
                        d.options.stroke_color.r,
                        d.options.stroke_color.g,
                        d.options.stroke_color.b,
                        d.options.stroke_color.a,
                    );
                    window.paint_path(path, color);
                }
            }
        }
    }
}

fn paint_text(
    t: &TextDrawable,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = (t.font_size * zoom) as f32;
    if font_size <= 0.0 {
        return;
    }
    let family = match t.font_family {
        crate::core::types::FontFamily::Virgil => crate::fonts::VIRGIL_FAMILY,
        crate::core::types::FontFamily::Helvetica => "Helvetica",
        crate::core::types::FontFamily::Cascadia => "Cascadia Code",
        crate::core::types::FontFamily::Assistant => "Assistant",
        crate::core::types::FontFamily::Excalifont => crate::fonts::EXCALIFONT_FAMILY,
        crate::core::types::FontFamily::ComicShanns => "Comic Sans MS",
        crate::core::types::FontFamily::Liberation => "Liberation Sans",
        crate::core::types::FontFamily::Nunito => "Nunito",
        crate::core::types::FontFamily::Lilita => "Lilita One",
        crate::core::types::FontFamily::Xiaolai => "Xiaolai SC",
    };
    let color = rgba_hex(t.color.r, t.color.g, t.color.b, t.color.a);
    let run = TextRun {
        len: t.text.len(),
        font: font(family),
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let text_system = window.text_system().clone();
    let lines = match text_system.shape_text(
        SharedString::from(t.text.as_str()),
        px(font_size),
        &[run],
        None,
        None,
    ) {
        Ok(lines) => lines,
        Err(_) => return,
    };

    let x = (scroll_x + t.x * zoom) as f32;
    let y = (scroll_y + t.y * zoom) as f32;
    let line_height = px((t.font_size * t.line_height * zoom) as f32);
    let align = match t.text_align {
        crate::core::types::TextAlign::Left => TextAlign::Left,
        crate::core::types::TextAlign::Center => TextAlign::Center,
        crate::core::types::TextAlign::Right => TextAlign::Right,
    };
    let align_width = Some(Bounds {
        origin: point(px(x), px(y)),
        size: size(px((t.width * zoom) as f32), px((t.height * zoom) as f32)),
    });

    let mut cursor = point(px(x), px(y));
    for line in lines.iter() {
        if line
            .paint(cursor, line_height, align, align_width, window, cx)
            .is_err()
        {
            return;
        }
        cursor.y += line_height;
    }
}

fn paint_image(
    img: &crate::render::shape::ImageDrawable,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    files: &HashMap<String, Arc<RenderImage>>,
    window: &mut Window,
) {
    let x = (scroll_x + img.x * zoom) as f32;
    let y = (scroll_y + img.y * zoom) as f32;
    let w = (img.width * zoom) as f32;
    let h = (img.height * zoom) as f32;
    if let Some(render) = img.file_id.as_deref().and_then(|id| files.get(id)) {
        let bounds = Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        };
        if window
            .paint_image(bounds, bounds, Corners::default(), render.clone(), 0, false)
            .is_ok()
        {
            return;
        }
    }
    let mut builder = PathBuilder::stroke(px(1.0));
    builder.move_to(point(px(x), px(y)));
    builder.line_to(point(px(x + w), px(y)));
    builder.line_to(point(px(x + w), px(y + h)));
    builder.line_to(point(px(x), px(y + h)));
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, rgb(0xced4da));
    }
    let cx_mid = x + w / 2.0;
    let cy_mid = y + h / 2.0;
    let mut cross1 = PathBuilder::stroke(px(1.0));
    cross1.move_to(point(px(x + 6.0), px(y + 6.0)));
    cross1.line_to(point(px(x + w - 6.0), px(y + h - 6.0)));
    if let Ok(path) = cross1.build() {
        window.paint_path(path, rgb(0xadb5bd));
    }
    let mut cross2 = PathBuilder::stroke(px(1.0));
    cross2.move_to(point(px(x + w - 6.0), px(y + 6.0)));
    cross2.line_to(point(px(x + 6.0), px(y + h - 6.0)));
    if let Ok(path) = cross2.build() {
        window.paint_path(path, rgb(0xadb5bd));
    }
    let _ = (cx_mid, cy_mid);
}

pub fn paint_grid(
    grid: f64,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    viewport: (f64, f64),
    window: &mut Window,
) {
    let step = grid * zoom;
    if step < 4.0 {
        return;
    }
    let (vw, vh) = viewport;
    let color = rgb(0xe9ecef);

    let k0 = ((0.0 - scroll_x) / step).ceil() as i64;
    let k1 = ((vw - scroll_x) / step).ceil() as i64;
    for k in k0..=k1 {
        let x = (scroll_x + k as f64 * step) as f32;
        let mut b = PathBuilder::stroke(px(1.0));
        b.move_to(point(px(x), px(0.0)));
        b.line_to(point(px(x), px(vh as f32)));
        if let Ok(path) = b.build() {
            window.paint_path(path, color);
        }
    }

    let j0 = ((0.0 - scroll_y) / step).ceil() as i64;
    let j1 = ((vh - scroll_y) / step).ceil() as i64;
    for j in j0..=j1 {
        let y = (scroll_y + j as f64 * step) as f32;
        let mut b = PathBuilder::stroke(px(1.0));
        b.move_to(point(px(0.0), px(y)));
        b.line_to(point(px(vw as f32), px(y)));
        if let Ok(path) = b.build() {
            window.paint_path(path, color);
        }
    }
}

pub fn paint_linear_handles(
    points: &[crate::core::geometry::Point],
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    let s = 4.0;
    for p in points {
        let x = (scroll_x + p.x * zoom) as f32;
        let y = (scroll_y + p.y * zoom) as f32;
        let (x0, y0, x1, y1) = (x - s, y - s, x + s, y + s);

        let mut fill = PathBuilder::fill();
        fill.move_to(point(px(x0), px(y0)));
        fill.line_to(point(px(x1), px(y0)));
        fill.line_to(point(px(x1), px(y1)));
        fill.line_to(point(px(x0), px(y1)));
        fill.close();
        if let Ok(path) = fill.build() {
            window.paint_path(path, rgb(0xffffff));
        }
        let mut edge = PathBuilder::stroke(px(1.5));
        edge.move_to(point(px(x0), px(y0)));
        edge.line_to(point(px(x1), px(y0)));
        edge.line_to(point(px(x1), px(y1)));
        edge.line_to(point(px(x0), px(y1)));
        edge.close();
        if let Ok(path) = edge.build() {
            window.paint_path(path, rgb(0x1971c2));
        }
    }
}

pub fn paint_guide(
    points: &[crate::core::geometry::Point],
    cursor: Option<crate::core::geometry::Point>,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    if points.is_empty() {
        return;
    }
    let mut b = PathBuilder::stroke(px(2.0));
    for (i, p) in points.iter().enumerate() {
        let x = (scroll_x + p.x * zoom) as f32;
        let y = (scroll_y + p.y * zoom) as f32;
        if i == 0 {
            b.move_to(point(px(x), px(y)));
        } else {
            b.line_to(point(px(x), px(y)));
        }
    }
    if let Some(c) = cursor {
        let x = (scroll_x + c.x * zoom) as f32;
        let y = (scroll_y + c.y * zoom) as f32;
        b.line_to(point(px(x), px(y)));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, rgb(0x4dabf7));
    }
    paint_linear_handles(points, zoom, scroll_x, scroll_y, window);
}

pub struct CanvasView {
    pub scene: Scene,
    pub zoom: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub image_sources: HashMap<String, Arc<RenderImage>>,
}

impl CanvasView {
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            image_sources: HashMap::new(),
        }
    }

    pub fn with_images(mut self, image_sources: HashMap<String, Arc<RenderImage>>) -> Self {
        self.image_sources = image_sources;
        self
    }
}

pub fn paint_selection_overlay(
    bounds: &crate::core::bounds::Bounds,
    color: Rgba,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    let sx = scroll_x + bounds.min_x * zoom;
    let sy = scroll_y + bounds.min_y * zoom;
    let ex = scroll_x + bounds.max_x * zoom;
    let ey = scroll_y + bounds.max_y * zoom;
    let cx = (sx + ex) / 2.0;
    let cy = (sy + ey) / 2.0;

    let mut rect = PathBuilder::stroke(px(1.0));
    rect.move_to(point(px(sx as f32), px(sy as f32)));
    rect.line_to(point(px(ex as f32), px(sy as f32)));
    rect.line_to(point(px(ex as f32), px(ey as f32)));
    rect.line_to(point(px(sx as f32), px(ey as f32)));
    rect.close();
    if let Ok(path) = rect.build() {
        window.paint_path(path, color);
    }

    let handle_size = 6.0;
    let handles = [
        (sx, sy),
        (cx, sy),
        (ex, sy),
        (ex, cy),
        (ex, ey),
        (cx, ey),
        (sx, ey),
        (sx, cy),
    ];
    for (hx, hy) in handles {
        let (x0, y0) = ((hx - handle_size) as f32, (hy - handle_size) as f32);
        let (x1, y1) = ((hx + handle_size) as f32, (hy + handle_size) as f32);

        let mut fill = PathBuilder::fill();
        fill.move_to(point(px(x0), px(y0)));
        fill.line_to(point(px(x1), px(y0)));
        fill.line_to(point(px(x1), px(y1)));
        fill.line_to(point(px(x0), px(y1)));
        fill.close();
        if let Ok(path) = fill.build() {
            window.paint_path(path, rgb(0xffffff));
        }

        let mut edge = PathBuilder::stroke(px(1.0));
        edge.move_to(point(px(x0), px(y0)));
        edge.line_to(point(px(x1), px(y0)));
        edge.line_to(point(px(x1), px(y1)));
        edge.line_to(point(px(x0), px(y1)));
        edge.close();
        if let Ok(path) = edge.build() {
            window.paint_path(path, color);
        }
    }

    let ry = sy - 24.0 * zoom;
    let radius = handle_size;
    let mut knob = PathBuilder::fill();
    for i in 0..24 {
        let t = i as f64 / 24.0 * std::f64::consts::TAU;
        let vx = (cx + radius * t.cos()) as f32;
        let vy = (ry + radius * t.sin()) as f32;
        if i == 0 {
            knob.move_to(point(px(vx), px(vy)));
        } else {
            knob.line_to(point(px(vx), px(vy)));
        }
    }
    knob.close();
    if let Ok(path) = knob.build() {
        window.paint_path(path, color);
    }
}

pub fn paint_marquee(
    bounds: &crate::core::bounds::Bounds,
    color: Rgba,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    let sx = scroll_x + bounds.min_x * zoom;
    let sy = scroll_y + bounds.min_y * zoom;
    let ex = scroll_x + bounds.max_x * zoom;
    let ey = scroll_y + bounds.max_y * zoom;

    let mut wash = PathBuilder::fill();
    wash.move_to(point(px(sx as f32), px(sy as f32)));
    wash.line_to(point(px(ex as f32), px(sy as f32)));
    wash.line_to(point(px(ex as f32), px(ey as f32)));
    wash.line_to(point(px(sx as f32), px(ey as f32)));
    wash.close();
    if let Ok(path) = wash.build() {
        window.paint_path(path, color.alpha(0.1));
    }

    let mut rect = PathBuilder::stroke(px(1.0));
    rect.move_to(point(px(sx as f32), px(sy as f32)));
    rect.line_to(point(px(ex as f32), px(sy as f32)));
    rect.line_to(point(px(ex as f32), px(ey as f32)));
    rect.line_to(point(px(sx as f32), px(ey as f32)));
    rect.close();
    if let Ok(path) = rect.build() {
        window.paint_path(path, color);
    }
}

/// Draws the lasso trail: a translucent wash over the traced polygon plus a
/// selection-colored outline, matching upstream's `LassoTrail` styling.
pub fn paint_lasso(
    points: &[crate::core::geometry::Point],
    color: Rgba,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    if points.len() < 2 {
        return;
    }
    if points.len() >= 3 {
        let mut wash = PathBuilder::fill();
        for (i, p) in points.iter().enumerate() {
            let x = (scroll_x + p.x * zoom) as f32;
            let y = (scroll_y + p.y * zoom) as f32;
            if i == 0 {
                wash.move_to(point(px(x), px(y)));
            } else {
                wash.line_to(point(px(x), px(y)));
            }
        }
        wash.close();
        if let Ok(path) = wash.build() {
            window.paint_path(path, color.alpha(0.05));
        }
    }

    let mut outline = PathBuilder::stroke(px(1.0));
    for (i, p) in points.iter().enumerate() {
        let x = (scroll_x + p.x * zoom) as f32;
        let y = (scroll_y + p.y * zoom) as f32;
        if i == 0 {
            outline.move_to(point(px(x), px(y)));
        } else {
            outline.line_to(point(px(x), px(y)));
        }
    }
    outline.close();
    if let Ok(path) = outline.build() {
        window.paint_path(path, color);
    }
}

pub fn paint_ink(
    points: &[crate::core::geometry::Point],
    stroke_width: f64,
    color: Rgba,
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    if points.is_empty() {
        return;
    }
    let outline = crate::render::shape::free_draw_outline(points, stroke_width, true);
    let mut ib = PathBuilder::fill();
    for (i, pt) in outline.iter().enumerate() {
        let x = (scroll_x + pt[0] * zoom) as f32;
        let y = (scroll_y + pt[1] * zoom) as f32;
        if i == 0 {
            ib.move_to(point(px(x), px(y)));
        } else {
            ib.line_to(point(px(x), px(y)));
        }
    }
    ib.close();
    if let Ok(path) = ib.build() {
        window.paint_path(path, color);
    }
}

/// Laser trail width, matching upstream's `size: 2` radius.
const LASER_WIDTH: f32 = 4.0;

/// Draws the laser trail. Every sample carries its remaining lifetime fraction
/// (1.0 freshly added, 0.0 gone); each segment is stroked at the older
/// endpoint's width so the trail retracts from its oldest end, mirroring
/// upstream's per-point size decay.
pub fn paint_laser(
    points: &[(crate::core::geometry::Point, f64)],
    zoom: f64,
    scroll_x: f64,
    scroll_y: f64,
    window: &mut Window,
) {
    let color = rgb(0xff0000);
    let coords: Vec<(f32, f32, f64)> = points
        .iter()
        .map(|(p, remaining)| {
            (
                (scroll_x + p.x * zoom) as f32,
                (scroll_y + p.y * zoom) as f32,
                *remaining,
            )
        })
        .collect();

    if coords.len() == 1 {
        let (x, y, remaining) = coords[0];
        paint_laser_dot(x, y, remaining, color, window);
        return;
    }

    for pair in coords.windows(2) {
        let (x0, y0, t0) = pair[0];
        let (x1, y1, t1) = pair[1];
        let width = (LASER_WIDTH as f64 * t0.min(t1)) as f32;
        if width < 0.5 {
            continue;
        }
        let mut builder = PathBuilder::stroke(px(width));
        builder.move_to(point(px(x0), px(y0)));
        builder.line_to(point(px(x1), px(y1)));
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

fn paint_laser_dot(x: f32, y: f32, remaining: f64, color: Rgba, window: &mut Window) {
    let radius = LASER_WIDTH as f64 / 2.0 * remaining;
    if radius < 0.5 {
        return;
    }
    let mut builder = PathBuilder::fill();
    for i in 0..16 {
        let theta = i as f64 / 16.0 * std::f64::consts::TAU;
        let vx = x + (radius * theta.cos()) as f32;
        let vy = y + (radius * theta.sin()) as f32;
        if i == 0 {
            builder.move_to(point(px(vx), px(vy)));
        } else {
            builder.line_to(point(px(vx), px(vy)));
        }
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

impl Render for CanvasView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let scene = self.scene.clone();
        let zoom = self.zoom;
        let scroll_x = self.scroll_x;
        let scroll_y = self.scroll_y;
        let images = self.image_sources.clone();

        canvas(
            move |_bounds, _window, _cx| (),
            move |_bounds, _state, window, cx| {
                paint_scene(&scene, zoom, scroll_x, scroll_y, &images, window, cx);
            },
        )
        .size_full()
    }
}
