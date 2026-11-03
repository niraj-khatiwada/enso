use crate::{
    core::{
        arrowhead::Arrowhead,
        element::{Element, ElementType, FreeDrawElement},
        geometry::Point,
        types::{
            FillStyle, FontFamily, Roundness, RoundnessType, StrokeStyle, TextAlign, VerticalAlign,
        },
    },
    render::rough::{self, PathSeg, RoughOptions, RoughSeed},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
            Self::new(r, g, b, 1.0)
        } else {
            Self::new(0.0, 0.0, 0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawOptions {
    pub seed: i64,
    pub roughness: f64,
    pub stroke_width: f64,
    pub stroke_color: Rgba,
    pub fill_color: Option<Rgba>,
    pub fill_style: FillStyle,
    pub stroke_style: StrokeStyle,
    pub opacity: f64,
    pub roundness: Option<Roundness>,
}

impl Default for DrawOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            roughness: 1.0,
            stroke_width: 2.0,
            stroke_color: Rgba::new(0.12, 0.12, 0.12, 1.0),
            fill_color: None,
            fill_style: FillStyle::Hachure,
            stroke_style: StrokeStyle::Solid,
            opacity: 100.0,
            roundness: None,
        }
    }
}

impl DrawOptions {
    pub fn from_element(element: &Element) -> Self {
        let base = element.base();
        let stroke_color = Rgba::from_hex(&base.stroke_color);
        let fill_color = if crate::core::color::is_transparent(&base.background_color) {
            None
        } else {
            Some(Rgba::from_hex(&base.background_color))
        };
        Self {
            seed: base.seed,
            roughness: adjust_roughness(element),
            stroke_width: base.stroke_width,
            stroke_color,
            fill_color,
            fill_style: base.fill_style,
            stroke_style: base.stroke_style,
            opacity: base.opacity,
            roundness: base.roundness,
        }
    }
}

fn adjust_roughness(element: &Element) -> f64 {
    let base = element.base();
    let roughness = base.roughness;
    let max_size = base.width.max(base.height);
    let min_size = base.width.min(base.height);
    let round_ok =
        min_size >= 15.0 && base.roundness.is_some() && can_change_roundness(element.kind());
    let long_linear = is_linear_element(element.kind()) && max_size >= 50.0;
    if (min_size >= 20.0 && max_size >= 50.0) || round_ok || long_linear {
        return roughness;
    }
    let divisor = if max_size < 10.0 { 3.0 } else { 2.0 };
    (roughness / divisor).min(2.5)
}

fn can_change_roundness(kind: ElementType) -> bool {
    matches!(
        kind,
        ElementType::Rectangle
            | ElementType::Iframe
            | ElementType::Embeddable
            | ElementType::Line
            | ElementType::Diamond
            | ElementType::StickyNote
            | ElementType::Image
    )
}

fn is_linear_element(kind: ElementType) -> bool {
    matches!(kind, ElementType::Line | ElementType::Arrow)
}

fn rough_options(options: &DrawOptions, continuous_path: bool, curve_fitting: f64) -> RoughOptions {
    RoughOptions::new(
        options.roughness,
        options.seed,
        options.stroke_style != StrokeStyle::Solid,
        continuous_path || options.roughness < 2.0,
        curve_fitting,
    )
}

fn stroke_options(options: &DrawOptions) -> DrawOptions {
    let mut o = *options;
    if o.stroke_style != StrokeStyle::Solid {
        o.stroke_width += 0.5;
    }
    o
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadraticTo(f64, f64, f64, f64),
    CubicTo(f64, f64, f64, f64, f64, f64),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    pub shape: ShapeKind,
    pub ops: Vec<Op>,
    pub options: DrawOptions,
    pub sets: Vec<Vec<Op>>,
    pub text: Option<TextDrawable>,
    pub image: Option<ImageDrawable>,
    pub fill_path: Option<Vec<Op>>,

    pub ink: Option<Vec<Op>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Freehand,
    Arrowhead,
    Text,
    Image,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextDrawable {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub font_family: FontFamily,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub color: Rgba,
    pub width: f64,
    pub height: f64,
    pub line_height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageDrawable {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub file_id: Option<String>,
}

pub fn ops_to_polygon(ops: &[Op]) -> Vec<Point> {
    let steps = 8;
    let mut out: Vec<Point> = Vec::new();
    let mut cur = Point::zero();
    for op in ops {
        match *op {
            Op::MoveTo(x, y) | Op::LineTo(x, y) => {
                cur = Point::new(x, y);
                push_unique(&mut out, cur);
            }
            Op::QuadraticTo(cx, cy, x, y) => {
                let c = Point::new(cx, cy);
                let end = Point::new(x, y);
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let mt = 1.0 - t;
                    push_unique(
                        &mut out,
                        Point::new(
                            mt * mt * cur.x + 2.0 * mt * t * c.x + t * t * end.x,
                            mt * mt * cur.y + 2.0 * mt * t * c.y + t * t * end.y,
                        ),
                    );
                }
                cur = end;
            }
            Op::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                let (c1, c2, end) = (Point::new(c1x, c1y), Point::new(c2x, c2y), Point::new(x, y));
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let mt = 1.0 - t;
                    push_unique(
                        &mut out,
                        Point::new(
                            mt * mt * mt * cur.x
                                + 3.0 * mt * mt * t * c1.x
                                + 3.0 * mt * t * t * c2.x
                                + t * t * t * end.x,
                            mt * mt * mt * cur.y
                                + 3.0 * mt * mt * t * c1.y
                                + 3.0 * mt * t * t * c2.y
                                + t * t * t * end.y,
                        ),
                    );
                }
                cur = end;
            }
            Op::Close => {}
        }
    }
    out
}

fn push_unique(v: &mut Vec<Point>, p: Point) {
    if let Some(last) = v.last()
        && (last.x - p.x).abs() < 1e-9
        && (last.y - p.y).abs() < 1e-9
    {
        return;
    }
    v.push(p);
}

pub fn hachure_over_polygon(
    poly: &[Point],
    gap: f64,
    angle_deg: f64,
    seed: i64,
    jitter: f64,
) -> Vec<Vec<Op>> {
    if poly.len() < 3 || gap <= 0.0 {
        return Vec::new();
    }
    let rad = angle_deg.to_radians();
    let (s, c) = (rad.sin(), rad.cos());

    let pts: Vec<Point> = poly
        .iter()
        .map(|p| Point::new(p.x * c + p.y * s, -p.x * s + p.y * c))
        .collect();
    let vmin = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let vmax = pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let mut rng = RoughSeed::new(seed);
    let mut sets = Vec::new();
    let mut v = vmin + gap * 0.5;
    while v < vmax {
        let jv = v + rng.range(-jitter, jitter);
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            if (a.y <= jv && b.y > jv) || (b.y <= jv && a.y > jv) {
                let t = (jv - a.y) / (b.y - a.y);
                xs.push(a.x + (b.x - a.x) * t);
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks(2) {
            if pair.len() < 2 || pair[1] - pair[0] < 1.0 {
                continue;
            }

            let p0 = Point::new(pair[0] * c - jv * s, pair[0] * s + jv * c);
            let p1 = Point::new(pair[1] * c - jv * s, pair[1] * s + jv * c);
            sets.push(vec![Op::MoveTo(p0.x, p0.y), Op::LineTo(p1.x, p1.y)]);
        }
        v += gap;
    }
    sets
}

pub fn rough_rectangle(x: f64, y: f64, w: f64, h: f64, options: &DrawOptions) -> Drawable {
    let continuous = options.roundness.is_some();
    let mut o = rough_options(options, continuous, 0.95);
    let outline = if let Some(roundness) = options.roundness {
        let r = corner_radius(w.min(h), Some(roundness));
        rough::svg_path(&rounded_rect_segments(x, y, w, h, r), &mut o)
    } else {
        let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
        rough::polygon(&pts, &mut o)
    };
    let outline = apply_stroke_style(outline, options.stroke_style);

    let r = corner_radius(w.min(h), options.roundness);
    let fill_path = if r > 0.0 {
        Some(rounded_rect_ops(x, y, x + w, y + h, r))
    } else {
        Some(vec![
            Op::MoveTo(x, y),
            Op::LineTo(x + w, y),
            Op::LineTo(x + w, y + h),
            Op::LineTo(x, y + h),
            Op::Close,
        ])
    };

    let fill_sets = if options.fill_color.is_some() {
        let poly = fill_path
            .as_ref()
            .map(|p| ops_to_polygon(p))
            .unwrap_or_default();
        fill_sets_for(&poly, options)
    } else {
        Vec::new()
    };

    let sets = if fill_sets.is_empty() {
        vec![outline.clone()]
    } else {
        let mut v = vec![outline.clone()];
        v.extend(fill_sets);
        v
    };

    Drawable {
        shape: ShapeKind::Rectangle,
        ops: outline,
        options: stroke_options(options),
        ink: None,
        text: None,
        image: None,
        sets,
        fill_path,
    }
}

fn corner_radius(dim: f64, roundness: Option<Roundness>) -> f64 {
    // Mirror upstream getCornerRadius so exported corners match the editor: legacy
    // and proportional corners take 25% of the shorter side, while adaptive corners
    // stay proportional until the side passes the 4x cutoff, then clamp to a fixed
    // radius (32px unless the element overrides it).
    const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
    const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;
    match roundness {
        None => 0.0,
        Some(r) => match r.kind {
            RoundnessType::Legacy | RoundnessType::ProportionalRadius => {
                dim * DEFAULT_PROPORTIONAL_RADIUS
            }
            RoundnessType::AdaptiveRadius => {
                let fixed = r.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
                if dim <= fixed / DEFAULT_PROPORTIONAL_RADIUS {
                    dim * DEFAULT_PROPORTIONAL_RADIUS
                } else {
                    fixed
                }
            }
        },
    }
}

fn quad_to_cubic(p0: (f64, f64), c: (f64, f64), p2: (f64, f64)) -> (f64, f64, f64, f64) {
    // Exact quadratic-to-cubic elevation, matching rough.js quadraticCurveTo so the
    // rounded rectangle outline traces the same curve as the upstream renderer.
    (
        p0.0 + (2.0 / 3.0) * (c.0 - p0.0),
        p0.1 + (2.0 / 3.0) * (c.1 - p0.1),
        p2.0 + (2.0 / 3.0) * (c.0 - p2.0),
        p2.1 + (2.0 / 3.0) * (c.1 - p2.1),
    )
}

fn rounded_rect_segments(x: f64, y: f64, w: f64, h: f64, r: f64) -> Vec<PathSeg> {
    let rr = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (x2, y2) = (x + w, y + h);
    let (a, b, c, d) = quad_to_cubic((x2 - rr, y), (x2, y), (x2, y + rr));
    let (e, f, g, h2) = quad_to_cubic((x2, y2 - rr), (x2, y2), (x2 - rr, y2));
    let (i, j, k, l) = quad_to_cubic((x + rr, y2), (x, y2), (x, y2 - rr));
    let (m, n, o, p) = quad_to_cubic((x, y + rr), (x, y), (x + rr, y));
    vec![
        PathSeg::Move(x + rr, y),
        PathSeg::Line(x2 - rr, y),
        PathSeg::Cubic(a, b, c, d, x2, y + rr),
        PathSeg::Line(x2, y2 - rr),
        PathSeg::Cubic(e, f, g, h2, x2 - rr, y2),
        PathSeg::Line(x + rr, y2),
        PathSeg::Cubic(i, j, k, l, x, y2 - rr),
        PathSeg::Line(x, y + rr),
        PathSeg::Cubic(m, n, o, p, x + rr, y),
    ]
}

fn diamond_segments(
    top: (f64, f64),
    right: (f64, f64),
    bottom: (f64, f64),
    left: (f64, f64),
    vr: f64,
    hr: f64,
) -> Vec<PathSeg> {
    vec![
        PathSeg::Move(top.0 + vr, top.1 + hr),
        PathSeg::Line(right.0 - vr, right.1 - hr),
        PathSeg::Cubic(
            right.0,
            right.1,
            right.0,
            right.1,
            right.0 - vr,
            right.1 + hr,
        ),
        PathSeg::Line(bottom.0 + vr, bottom.1 - hr),
        PathSeg::Cubic(
            bottom.0,
            bottom.1,
            bottom.0,
            bottom.1,
            bottom.0 - vr,
            bottom.1 - hr,
        ),
        PathSeg::Line(left.0 + vr, left.1 + hr),
        PathSeg::Cubic(left.0, left.1, left.0, left.1, left.0 + vr, left.1 - hr),
        PathSeg::Line(top.0 - vr, top.1 + hr),
        PathSeg::Cubic(top.0, top.1, top.0, top.1, top.0 + vr, top.1 + hr),
    ]
}

fn rounded_rect_ops(x1: f64, y1: f64, x2: f64, y2: f64, r: f64) -> Vec<Op> {
    if r <= 0.0 {
        return Vec::new();
    }
    let rr = r.min((x2 - x1).abs() / 2.0).min((y2 - y1).abs() / 2.0);
    if rr <= 0.0 {
        return Vec::new();
    }
    vec![
        Op::MoveTo(x1 + rr, y1),
        Op::LineTo(x2 - rr, y1),
        Op::QuadraticTo(x2, y1, x2, y1 + rr),
        Op::LineTo(x2, y2 - rr),
        Op::QuadraticTo(x2, y2, x2 - rr, y2),
        Op::LineTo(x1 + rr, y2),
        Op::QuadraticTo(x1, y2, x1, y2 - rr),
        Op::LineTo(x1, y1 + rr),
        Op::QuadraticTo(x1, y1, x1 + rr, y1),
        Op::Close,
    ]
}

fn fill_sets_for(poly: &[Point], options: &DrawOptions) -> Vec<Vec<Op>> {
    match options.fill_style {
        FillStyle::Hachure | FillStyle::CrossHatch => hachure_fill(poly, options),
        FillStyle::Zigzag => zigzag_fill(poly, options),
        FillStyle::Solid => Vec::new(),
    }
}

fn hachure_fill(poly: &[Point], options: &DrawOptions) -> Vec<Vec<Op>> {
    let angle = -41.0;
    let gap = 6.0 + options.roughness * 3.0;
    let jitter = options.roughness * 0.7;
    if options.fill_style == FillStyle::CrossHatch {
        let mut sets = hachure_over_polygon(poly, gap, angle, options.seed + 100, jitter);
        sets.extend(hachure_over_polygon(
            poly,
            gap,
            angle + 90.0,
            options.seed + 150,
            jitter,
        ));
        sets
    } else {
        hachure_over_polygon(poly, gap, angle, options.seed + 100, jitter)
    }
}

fn zigzag_fill(poly: &[Point], options: &DrawOptions) -> Vec<Vec<Op>> {
    let mut sets = Vec::new();
    if poly.len() < 3 {
        return sets;
    }
    let mut rng = RoughSeed::new(options.seed + 200);
    let gap = 7.0 + options.roughness * 3.0;
    let ymin = poly.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let ymax = poly.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let mut y = ymin + gap * 0.5;
    let mut flip = false;
    while y < ymax {
        let jy = y + rng.range(-options.roughness, options.roughness);

        let mut xs: Vec<f64> = Vec::new();
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            if (a.y <= jy && b.y > jy) || (b.y <= jy && a.y > jy) {
                let t = (jy - a.y) / (b.y - a.y);
                xs.push(a.x + (b.x - a.x) * t);
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks(2) {
            if pair.len() < 2 || pair[1] - pair[0] < 2.0 {
                continue;
            }
            let (x0, x1) = (pair[0], pair[1]);
            let mut line = vec![Op::MoveTo(x0, jy)];
            let mut xx = x0;
            while xx < x1 {
                let nx = (xx + gap).min(x1);
                let mid = jy + if flip { -gap / 2.0 } else { gap / 2.0 };
                line.push(Op::LineTo(nx, mid));
                line.push(Op::LineTo(nx, jy));
                xx = nx;
            }
            sets.push(line);
        }
        flip = !flip;
        y += gap;
    }
    sets
}

pub fn rough_ellipse(x: f64, y: f64, w: f64, h: f64, options: &DrawOptions) -> Drawable {
    let rx = w / 2.0;
    let ry = h / 2.0;
    let cx = x + rx;
    let cy = y + ry;

    let mut o = rough_options(options, false, 1.0);
    let outline = rough::ellipse_ops(cx, cy, w, h, &mut o);
    let outline = apply_stroke_style(outline, options.stroke_style);

    let fill_path = {
        let steps = 48;
        let mut p = Vec::with_capacity(steps + 1);
        for i in 0..steps {
            let t = i as f64 / steps as f64;
            let angle = t * std::f64::consts::TAU;
            let px = cx + rx * angle.cos();
            let py = cy + ry * angle.sin();
            if i == 0 {
                p.push(Op::MoveTo(px, py));
            } else {
                p.push(Op::LineTo(px, py));
            }
        }
        p.push(Op::Close);
        Some(p)
    };

    let fill_sets = if options.fill_color.is_some() {
        let poly = fill_path
            .as_ref()
            .map(|p| ops_to_polygon(p))
            .unwrap_or_default();
        fill_sets_for(&poly, options)
    } else {
        Vec::new()
    };

    let sets = if fill_sets.is_empty() {
        vec![outline.clone()]
    } else {
        let mut v = vec![outline.clone()];
        v.extend(fill_sets);
        v
    };

    Drawable {
        shape: ShapeKind::Ellipse,
        ops: outline,
        options: stroke_options(options),
        ink: None,
        text: None,
        image: None,
        sets,
        fill_path,
    }
}

pub fn rough_diamond(x: f64, y: f64, w: f64, h: f64, options: &DrawOptions) -> Drawable {
    let continuous = options.roundness.is_some();
    let mut o = rough_options(options, continuous, 0.95);
    let top = (x + w / 2.0, y);
    let right = (x + w, y + h / 2.0);
    let bottom = (x + w / 2.0, y + h);
    let left = (x, y + h / 2.0);
    let outline = if let Some(roundness) = options.roundness {
        let vr = corner_radius((top.0 - left.0).abs(), Some(roundness));
        let hr = corner_radius((right.1 - top.1).abs(), Some(roundness));
        rough::svg_path(&diamond_segments(top, right, bottom, left, vr, hr), &mut o)
    } else {
        rough::polygon(&[top, right, bottom, left], &mut o)
    };
    let outline = apply_stroke_style(outline, options.stroke_style);

    let fill_path = Some(vec![
        Op::MoveTo(x + w / 2.0, y),
        Op::LineTo(x + w, y + h / 2.0),
        Op::LineTo(x + w / 2.0, y + h),
        Op::LineTo(x, y + h / 2.0),
        Op::Close,
    ]);

    let fill_sets = if options.fill_color.is_some() {
        let poly = fill_path
            .as_ref()
            .map(|p| ops_to_polygon(p))
            .unwrap_or_default();
        fill_sets_for(&poly, options)
    } else {
        Vec::new()
    };

    let sets = if fill_sets.is_empty() {
        vec![outline.clone()]
    } else {
        let mut v = vec![outline.clone()];
        v.extend(fill_sets);
        v
    };

    Drawable {
        shape: ShapeKind::Diamond,
        ops: outline,
        options: stroke_options(options),
        ink: None,
        text: None,
        image: None,
        sets,
        fill_path,
    }
}

pub fn rough_line(points: &[Point], options: &DrawOptions) -> Drawable {
    let mut o = rough_options(options, false, 0.95);
    let pts: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
    let outline = if options.roundness.is_some() {
        rough::curve(&pts, &mut o)
    } else {
        rough::linear_path(&pts, false, &mut o)
    };
    let outline = apply_stroke_style(outline, options.stroke_style);
    Drawable {
        shape: ShapeKind::Line,
        sets: vec![outline.clone()],
        ops: outline,
        options: stroke_options(options),
        text: None,
        image: None,
        fill_path: None,
        ink: None,
    }
}

fn apply_stroke_style(ops: Vec<Op>, style: StrokeStyle) -> Vec<Op> {
    if style == StrokeStyle::Solid {
        return ops;
    }
    let (dash, gap) = if style == StrokeStyle::Dotted {
        (2.0, 2.0)
    } else {
        (8.0, 6.0)
    };
    let mut out = Vec::new();
    for subpath in flatten_subpaths(&ops) {
        dash_polyline(&subpath, dash, gap, &mut out);
    }
    out
}

fn flatten_subpaths(ops: &[Op]) -> Vec<Vec<(f64, f64)>> {
    const STEPS: usize = 8;
    let mut subpaths: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    let mut pos = (0.0, 0.0);
    for op in ops {
        match *op {
            Op::MoveTo(x, y) => {
                push_subpath(&mut subpaths, &mut cur);
                cur.push((x, y));
                pos = (x, y);
            }
            Op::LineTo(x, y) => {
                cur.push((x, y));
                pos = (x, y);
            }
            Op::QuadraticTo(cx, cy, x, y) => {
                for i in 1..=STEPS {
                    let t = i as f64 / STEPS as f64;
                    let mt = 1.0 - t;
                    cur.push((
                        mt * mt * pos.0 + 2.0 * mt * t * cx + t * t * x,
                        mt * mt * pos.1 + 2.0 * mt * t * cy + t * t * y,
                    ));
                }
                pos = (x, y);
            }
            Op::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                let start = pos;
                for i in 1..=STEPS {
                    let t = i as f64 / STEPS as f64;
                    let mt = 1.0 - t;
                    cur.push((
                        mt * mt * mt * start.0
                            + 3.0 * mt * mt * t * c1x
                            + 3.0 * mt * t * t * c2x
                            + t * t * t * x,
                        mt * mt * mt * start.1
                            + 3.0 * mt * mt * t * c1y
                            + 3.0 * mt * t * t * c2y
                            + t * t * t * y,
                    ));
                }
                pos = (x, y);
            }
            Op::Close => push_subpath(&mut subpaths, &mut cur),
        }
    }
    push_subpath(&mut subpaths, &mut cur);
    subpaths
}

fn push_subpath(subpaths: &mut Vec<Vec<(f64, f64)>>, cur: &mut Vec<(f64, f64)>) {
    if cur.len() >= 2 {
        subpaths.push(std::mem::take(cur));
    } else {
        cur.clear();
    }
}

fn dash_polyline(pts: &[(f64, f64)], dash: f64, gap: f64, out: &mut Vec<Op>) {
    if pts.len() < 2 {
        return;
    }
    out.push(Op::MoveTo(pts[0].0, pts[0].1));
    let mut drawing = true;
    let mut to_toggle = dash;
    for pair in pts.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let seg = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        if seg <= f64::EPSILON {
            continue;
        }
        let (dx, dy) = ((x1 - x0) / seg, (y1 - y0) / seg);
        if to_toggle <= 0.0 {
            drawing = !drawing;
            to_toggle = if drawing { dash } else { gap };
            if drawing {
                out.push(Op::MoveTo(x0, y0));
            }
        }
        let mut t = 0.0;
        while t < seg {
            let remaining = seg - t;
            if to_toggle < remaining {
                t += to_toggle;
                let px = x0 + dx * t;
                let py = y0 + dy * t;
                if drawing {
                    out.push(Op::LineTo(px, py));
                }
                drawing = !drawing;
                to_toggle = if drawing { dash } else { gap };
                if drawing {
                    out.push(Op::MoveTo(px, py));
                }
            } else {
                t = seg;
                to_toggle -= remaining;
                if drawing {
                    out.push(Op::LineTo(x1, y1));
                }
            }
        }
    }
}

pub fn free_draw_outline(
    points: &[Point],
    stroke_width: f64,
    simulate_pressure: bool,
) -> Vec<[f64; 2]> {
    let inputs: Vec<perfect_freehand::InputPoint> = points
        .iter()
        .map(|p| perfect_freehand::InputPoint::Array([p.x, p.y], None))
        .collect();
    perfect_freehand::get_stroke(
        &inputs,
        &perfect_freehand::StrokeOptions {
            size: Some(stroke_width * 4.25),
            thinning: Some(0.6),
            smoothing: Some(0.5),
            streamline: Some(0.5),
            simulate_pressure: Some(simulate_pressure),
            last: Some(true),
            ..Default::default()
        },
    )
}

pub fn perfect_freehand_drawable(f: &FreeDrawElement, options: &DrawOptions) -> Drawable {
    let outline = free_draw_outline(&f.points, options.stroke_width, f.simulate_pressure);
    let mut ink: Vec<Op> = outline
        .iter()
        .enumerate()
        .map(|(i, pt)| {
            if i == 0 {
                Op::MoveTo(pt[0], pt[1])
            } else {
                Op::LineTo(pt[0], pt[1])
            }
        })
        .collect();
    ink.push(Op::Close);
    Drawable {
        shape: ShapeKind::Freehand,
        ops: Vec::new(),
        sets: Vec::new(),
        options: *options,
        text: None,
        image: None,
        fill_path: None,
        ink: Some(ink),
    }
}

pub fn render_element(element: &Element) -> Vec<Drawable> {
    let base = element.base();
    let opts = DrawOptions::from_element(element);
    let x = base.x;
    let y = base.y;
    let w = base.width;
    let h = base.height;

    match element.kind() {
        ElementType::Rectangle | ElementType::StickyNote => {
            vec![rough_rectangle(x, y, w, h, &opts)]
        }
        ElementType::Embeddable | ElementType::Iframe => vec![
            rough_rectangle(x, y, w, h, &opts),
            embeddable_label_drawable(base, element.kind() == ElementType::Iframe, &opts),
        ],
        ElementType::Frame | ElementType::MagicFrame => {
            let mut v = vec![rough_rectangle(x, y, w, h, &opts)];
            if let Element::Frame(f) | Element::MagicFrame(f) = element {
                let name = f.name.clone().unwrap_or_else(|| "Frame".to_string());
                v.push(frame_label_drawable(x, y, &name, &opts));
            }
            v
        }
        ElementType::Ellipse => vec![rough_ellipse(x, y, w, h, &opts)],
        ElementType::Diamond => vec![rough_diamond(x, y, w, h, &opts)],
        ElementType::Line => {
            if let Element::Line(l) = element {
                let mut main = rough_line(&l.linear.points, &opts);
                let points = &l.linear.points;
                if l.polygon && points.len() >= 3 && opts.fill_color.is_some() {
                    let mut ring = Vec::with_capacity(points.len() + 1);
                    ring.push(Op::MoveTo(points[0].x, points[0].y));
                    for point in &points[1..] {
                        ring.push(Op::LineTo(point.x, point.y));
                    }
                    ring.push(Op::Close);
                    let poly = ops_to_polygon(&ring);
                    main.sets.extend(fill_sets_for(&poly, &opts));
                    main.fill_path = Some(ring);
                }
                let mut drawables = vec![main];
                if points.len() >= 2 {
                    if let Some(head) = l.linear.end_arrowhead {
                        drawables.push(arrowhead_drawable(
                            points[points.len() - 2],
                            points[points.len() - 1],
                            head,
                            &opts,
                        ));
                    }
                    if let Some(head) = l.linear.start_arrowhead {
                        drawables.push(arrowhead_drawable(points[1], points[0], head, &opts));
                    }
                }
                drawables
            } else {
                Vec::new()
            }
        }
        ElementType::Arrow => {
            if let Element::Arrow(a) = element {
                let mut drawables = vec![rough_line(&a.linear.points, &opts)];
                let points = &a.linear.points;
                if points.len() >= 2 {
                    if let Some(head) = a.linear.end_arrowhead {
                        drawables.push(arrowhead_drawable(
                            points[points.len() - 2],
                            points[points.len() - 1],
                            head,
                            &opts,
                        ));
                    }
                    if let Some(head) = a.linear.start_arrowhead {
                        drawables.push(arrowhead_drawable(points[1], points[0], head, &opts));
                    }
                }
                drawables
            } else {
                Vec::new()
            }
        }
        ElementType::Freedraw => {
            if let Element::Freedraw(f) = element {
                vec![perfect_freehand_drawable(f, &opts)]
            } else {
                Vec::new()
            }
        }
        ElementType::Text => {
            if let Element::Text(t) = element {
                vec![text_drawable(t)]
            } else {
                Vec::new()
            }
        }
        ElementType::Image => {
            if let Element::Image(img) = element {
                vec![image_drawable(img)]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Upstream punches a hole in an arrow's stroke where its bound text label sits
/// (`BOUND_TEXT_PADDING` in `renderElement.ts`). Our arrows are polylines, so
/// the same effect is achieved by skipping the covered parts of the path.
const BOUND_TEXT_PADDING: f64 = 5.0;

/// Renders `element` with the rest of the scene available, which lets arrow
/// labels cut their arrow the way upstream clips it.
pub fn render_element_with_lookup(
    element: &Element,
    lookup: &std::collections::HashMap<&str, &Element>,
) -> Vec<Drawable> {
    if let Element::Arrow(arrow) = element
        && let Some(hole) = arrow_label_hole(element, lookup)
    {
        let options = DrawOptions::from_element(element);
        let mut drawables = Vec::new();
        for piece in split_outside(&arrow.linear.points, &hole) {
            drawables.push(rough_line(&piece, &options));
        }
        let points = &arrow.linear.points;
        if points.len() >= 2 {
            if let Some(head) = arrow.linear.end_arrowhead {
                drawables.push(arrowhead_drawable(
                    points[points.len() - 2],
                    points[points.len() - 1],
                    head,
                    &options,
                ));
            }
            if let Some(head) = arrow.linear.start_arrowhead {
                drawables.push(arrowhead_drawable(points[1], points[0], head, &options));
            }
        }
        return drawables;
    }
    render_element(element)
}

fn arrow_label_hole(
    element: &Element,
    lookup: &std::collections::HashMap<&str, &Element>,
) -> Option<crate::core::bounds::Bounds> {
    let bound = element.base().bound_elements.as_ref()?;
    bound.iter().find_map(|item| {
        if item.kind != crate::core::element::BoundElementType::Text {
            return None;
        }
        match lookup.get(item.id.as_str()).copied()? {
            Element::Text(t) => Some(
                crate::core::bounds::Bounds::new(
                    t.base.x,
                    t.base.y,
                    t.base.x + t.base.width,
                    t.base.y + t.base.height,
                )
                .expand(BOUND_TEXT_PADDING),
            ),
            _ => None,
        }
    })
}

/// Splits a polyline into the pieces outside `hole`, joining sub-segments that
/// meet so a straight run across the hole becomes two continuous strokes.
fn split_outside(points: &[Point], hole: &crate::core::bounds::Bounds) -> Vec<Vec<Point>> {
    let mut visible: Vec<(Point, Point)> = Vec::new();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        match segment_inside(a, b, hole) {
            Some((t0, t1)) if t1 > t0 => {
                if t0 > 0.0 {
                    visible.push((a, a.lerp(b, t0)));
                }
                if t1 < 1.0 {
                    visible.push((a.lerp(b, t1), b));
                }
            }
            _ => visible.push((a, b)),
        }
    }

    let mut pieces: Vec<Vec<Point>> = Vec::new();
    for (a, b) in visible {
        if let Some(last) = pieces.last_mut()
            && last.last().is_some_and(|end| end.distance(a) < 1e-6)
        {
            last.push(b);
            continue;
        }
        pieces.push(vec![a, b]);
    }
    pieces
}

/// Liang-Barsky clip of `a`-`b` against `rect`, returning the `[t0, t1]`
/// interval lying inside it.
fn segment_inside(a: Point, b: Point, rect: &crate::core::bounds::Bounds) -> Option<(f64, f64)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;
    for (p, q) in [
        (-dx, a.x - rect.min_x),
        (dx, rect.max_x - a.x),
        (-dy, a.y - rect.min_y),
        (dy, rect.max_y - a.y),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            t0 = t0.max(r);
        } else {
            if r < t0 {
                return None;
            }
            t1 = t1.min(r);
        }
    }
    Some((t0, t1))
}

fn frame_label_drawable(x: f64, y: f64, name: &str, options: &DrawOptions) -> Drawable {
    let font_size = 14.0;
    Drawable {
        shape: ShapeKind::Text,
        ops: Vec::new(),
        sets: Vec::new(),
        options: *options,
        text: Some(TextDrawable {
            text: name.to_string(),
            x,
            y: y - font_size - 6.0,
            font_size,
            font_family: FontFamily::Helvetica,
            text_align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            color: options.stroke_color,
            width: name.chars().count() as f64 * font_size * 0.6,
            height: font_size * 1.4,
            line_height: 1.25,
        }),
        image: None,
        fill_path: None,
        ink: None,
    }
}

/// Upstream's pre-validation stand-in for web embeds, drawn in place of the
/// iframe it cannot render: the link, or "Empty Web-Embed" when there is none.
fn embeddable_label_drawable(
    base: &crate::core::element::ElementBase,
    is_iframe: bool,
    options: &DrawOptions,
) -> Drawable {
    let text = if is_iframe {
        "IFrame element".to_string()
    } else {
        base.link
            .clone()
            .filter(|link| !link.is_empty())
            .unwrap_or_else(|| "Empty Web-Embed".to_string())
    };
    let chars = text.chars().count().max(1) as f64;
    let font_size = (base.width / 2.0)
        .min(base.width / chars)
        .max(base.width / 30.0);
    let width = (base.width - 20.0).max(1.0);
    let wrapped = crate::core::text::wrap_text(&text, width, font_size);
    let lines = wrapped.lines().count().max(1) as f64;
    let height = lines * font_size * 1.25;
    let color = if crate::core::color::is_transparent(&base.stroke_color) {
        Rgba::new(0.0, 0.0, 0.0, 1.0)
    } else {
        options.stroke_color
    };
    Drawable {
        shape: ShapeKind::Text,
        ops: Vec::new(),
        sets: Vec::new(),
        options: *options,
        text: Some(TextDrawable {
            text: wrapped,
            x: base.x + 10.0,
            y: base.y + base.height / 2.0 - height / 2.0,
            font_size,
            font_family: FontFamily::Helvetica,
            text_align: TextAlign::Center,
            vertical_align: VerticalAlign::Top,
            color,
            width,
            height,
            line_height: 1.25,
        }),
        image: None,
        fill_path: None,
        ink: None,
    }
}

fn text_drawable(t: &crate::core::element::TextElement) -> Drawable {
    let base = &t.base;
    let color = Rgba::from_hex(&base.stroke_color);

    let text = if t.container_id.is_some() {
        crate::core::text::wrap_text(&t.text, base.width, t.font_size)
    } else {
        t.text.clone()
    };
    Drawable {
        shape: ShapeKind::Text,
        ops: Vec::new(),
        sets: Vec::new(),
        options: DrawOptions::from_element(&Element::Text(t.clone())),
        text: Some(TextDrawable {
            text,
            x: base.x,
            y: base.y,
            font_size: t.font_size,
            font_family: t.font_family,
            text_align: t.text_align,
            vertical_align: t.vertical_align,
            color,
            width: base.width,
            height: base.height,
            line_height: t.line_height,
        }),
        image: None,
        fill_path: None,
        ink: None,
    }
}

fn image_drawable(img: &crate::core::element::ImageElement) -> Drawable {
    let base = &img.base;
    Drawable {
        shape: ShapeKind::Image,
        ops: Vec::new(),
        sets: Vec::new(),
        options: DrawOptions::from_element(&Element::Image(img.clone())),
        text: None,
        image: Some(ImageDrawable {
            x: base.x,
            y: base.y,
            width: base.width,
            height: base.height,
            file_id: img.file_id.clone(),
        }),
        fill_path: None,
        ink: None,
    }
}

fn arrowhead_drawable(from: Point, to: Point, head: Arrowhead, options: &DrawOptions) -> Drawable {
    let dir = (to - from).normalized();
    let normal = Point::new(-dir.y, dir.x);
    let size = (options.stroke_width * 6.0).max(12.0);
    let tip = to;
    let seed = options.seed + 500;
    let preserve = options.roughness < 2.0;

    let options_for = |roughness_cap: f64, curve_fitting: f64| {
        RoughOptions::new(
            options.roughness.min(roughness_cap),
            seed,
            true,
            preserve,
            curve_fitting,
        )
    };

    let ops = match head {
        Arrowhead::Bar => {
            let base = tip - dir * (size * 0.5);
            let left = base + normal * (size * 0.5);
            let right = base - normal * (size * 0.5);
            let mut o = options_for(1.0, 0.95);
            rough::double_line(left.x, left.y, right.x, right.y, &mut o)
        }
        Arrowhead::Circle | Arrowhead::CircleOutline | Arrowhead::Dot => {
            let c = tip - dir * (size * 0.3);
            let r = if head == Arrowhead::Dot {
                size * 0.25
            } else {
                size * 0.3
            };
            let mut o = options_for(0.5, 1.0);
            rough::ellipse_ops(c.x, c.y, r * 2.0, r * 2.0, &mut o)
        }
        Arrowhead::Diamond | Arrowhead::DiamondOutline => {
            let c = tip - dir * (size * 0.6);
            let up = c + normal * (size * 0.4);
            let down = c - normal * (size * 0.4);
            let back = c - dir * (size * 0.4);
            let front = c + dir * (size * 0.4);
            let mut o = options_for(1.0, 0.95);
            rough::polygon(
                &[
                    (up.x, up.y),
                    (front.x, front.y),
                    (down.x, down.y),
                    (back.x, back.y),
                ],
                &mut o,
            )
        }
        Arrowhead::CardinalityOne
        | Arrowhead::CardinalityMany
        | Arrowhead::CardinalityOneOrMany
        | Arrowhead::CardinalityExactlyOne
        | Arrowhead::CardinalityZeroOrOne
        | Arrowhead::CardinalityZeroOrMany
        | Arrowhead::CrowfootOne
        | Arrowhead::CrowfootMany
        | Arrowhead::CrowfootOneOrMany => {
            let base = tip - dir * size;
            let left = base + normal * (size * 0.45);
            let right = base - normal * (size * 0.45);
            let mut o = options_for(1.0, 0.95);
            let mut ops = rough::double_line(left.x, left.y, tip.x, tip.y, &mut o);
            ops.extend(rough::double_line(right.x, right.y, tip.x, tip.y, &mut o));
            ops
        }

        _ => {
            let base = tip - dir * size;
            let left = base + normal * (size * 0.45);
            let right = base - normal * (size * 0.45);
            let mut o = options_for(1.0, 0.95);
            rough::polygon(
                &[(tip.x, tip.y), (left.x, left.y), (right.x, right.y)],
                &mut o,
            )
        }
    };

    Drawable {
        shape: ShapeKind::Arrowhead,
        sets: vec![ops.clone()],
        ops,
        options: *options,
        text: None,
        image: None,
        fill_path: None,
        ink: None,
    }
}
