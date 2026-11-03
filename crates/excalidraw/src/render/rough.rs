use std::f64::consts::{FRAC_PI_2, TAU};

use crate::render::shape::Op;

type PointRing = Vec<(f64, f64)>;

#[derive(Debug, Clone)]
pub struct RoughRandom {
    seed: i64,
}

impl RoughRandom {
    pub fn new(seed: i64) -> Self {
        Self { seed }
    }

    pub fn next_random(&mut self) -> f64 {
        if self.seed == 0 {
            return 0.0;
        }
        let s = (self.seed as i32).wrapping_mul(48271);
        self.seed = (s & 0x7fff_ffff) as i64;
        (self.seed as f64) / 2_147_483_648.0
    }
}

#[derive(Debug, Clone)]
pub struct RoughOptions {
    pub roughness: f64,
    pub seed: i64,
    pub max_randomness_offset: f64,
    pub bowing: f64,
    pub curve_tightness: f64,
    pub curve_fitting: f64,
    pub curve_step_count: f64,
    pub disable_multi_stroke: bool,
    pub preserve_vertices: bool,
    randomizer: RoughRandom,
}

impl RoughOptions {
    pub fn new(
        roughness: f64,
        seed: i64,
        disable_multi_stroke: bool,
        preserve_vertices: bool,
        curve_fitting: f64,
    ) -> Self {
        Self {
            roughness,
            seed,
            max_randomness_offset: 2.0,
            bowing: 1.0,
            curve_tightness: 0.0,
            curve_fitting,
            curve_step_count: 9.0,
            disable_multi_stroke,
            preserve_vertices,
            randomizer: RoughRandom::new(seed),
        }
    }

    fn second_pass(&self) -> Self {
        let seed = if self.seed != 0 { self.seed + 1 } else { 0 };
        let mut o = self.clone();
        o.seed = seed;
        o.randomizer = RoughRandom::new(seed);
        o
    }
}

fn random(o: &mut RoughOptions) -> f64 {
    o.randomizer.next_random()
}

fn offset(min: f64, max: f64, o: &mut RoughOptions, roughness_gain: f64) -> f64 {
    o.roughness * roughness_gain * (random(o) * (max - min) + min)
}

fn offset_opt(x: f64, o: &mut RoughOptions, roughness_gain: f64) -> f64 {
    offset(-x, x, o, roughness_gain)
}

fn line(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &mut RoughOptions,
    move_to: bool,
    overlay: bool,
) -> Vec<Op> {
    let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    let length = length_sq.sqrt();
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        -0.0016668 * length + 1.233334
    };
    let mut offset = o.max_randomness_offset;
    if offset * offset * 100.0 > length_sq {
        offset = length / 10.0;
    }
    let half_offset = offset / 2.0;
    let diverge_point = 0.2 + random(o) * 0.2;

    let mut mid_x = o.bowing * o.max_randomness_offset * (y2 - y1) / 200.0;
    let mut mid_y = o.bowing * o.max_randomness_offset * (x1 - x2) / 200.0;
    mid_x = offset_opt(mid_x, o, roughness_gain);
    mid_y = offset_opt(mid_y, o, roughness_gain);

    let mut ops = Vec::new();
    if move_to {
        let (dx, dy) = if o.preserve_vertices {
            (0.0, 0.0)
        } else {
            let pick = |o: &mut RoughOptions| {
                if overlay {
                    offset_opt(half_offset, o, roughness_gain)
                } else {
                    offset_opt(offset, o, roughness_gain)
                }
            };
            (pick(o), pick(o))
        };
        ops.push(Op::MoveTo(x1 + dx, y1 + dy));
    }

    let rand_off = |o: &mut RoughOptions| {
        if overlay {
            offset_opt(half_offset, o, roughness_gain)
        } else {
            offset_opt(offset, o, roughness_gain)
        }
    };
    let c1x = mid_x + x1 + (x2 - x1) * diverge_point + rand_off(o);
    let c1y = mid_y + y1 + (y2 - y1) * diverge_point + rand_off(o);
    let c2x = mid_x + x1 + 2.0 * (x2 - x1) * diverge_point + rand_off(o);
    let c2y = mid_y + y1 + 2.0 * (y2 - y1) * diverge_point + rand_off(o);
    let (ex, ey) = if o.preserve_vertices {
        (x2, y2)
    } else {
        (x2 + rand_off(o), y2 + rand_off(o))
    };
    ops.push(Op::CubicTo(c1x, c1y, c2x, c2y, ex, ey));
    ops
}

pub fn double_line(x1: f64, y1: f64, x2: f64, y2: f64, o: &mut RoughOptions) -> Vec<Op> {
    let mut ops = line(x1, y1, x2, y2, o, true, false);
    if !o.disable_multi_stroke {
        ops.extend(line(x1, y1, x2, y2, o, true, true));
    }
    ops
}

pub fn linear_path(points: &[(f64, f64)], close: bool, o: &mut RoughOptions) -> Vec<Op> {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::new();
        for pair in points.windows(2) {
            ops.extend(double_line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, o));
        }
        if close {
            let last = points[len - 1];
            let first = points[0];
            ops.extend(double_line(last.0, last.1, first.0, first.1, o));
        }
        ops
    } else if len == 2 {
        double_line(points[0].0, points[0].1, points[1].0, points[1].1, o)
    } else {
        Vec::new()
    }
}

pub fn polygon(points: &[(f64, f64)], o: &mut RoughOptions) -> Vec<Op> {
    linear_path(points, true, o)
}

fn curve_raw(
    points: &[(f64, f64)],
    close_point: Option<(f64, f64)>,
    o: &mut RoughOptions,
) -> Vec<Op> {
    let len = points.len();
    let mut ops = Vec::new();
    if len > 3 {
        let s = 1.0 - o.curve_tightness;
        ops.push(Op::MoveTo(points[1].0, points[1].1));
        let mut i = 1;
        while i + 2 < len {
            let p = points[i];
            let b1 = (
                p.0 + (s * points[i + 1].0 - s * points[i - 1].0) / 6.0,
                p.1 + (s * points[i + 1].1 - s * points[i - 1].1) / 6.0,
            );
            let b2 = (
                points[i + 1].0 + (s * p.0 - s * points[i + 2].0) / 6.0,
                points[i + 1].1 + (s * p.1 - s * points[i + 2].1) / 6.0,
            );
            let b3 = points[i + 1];
            ops.push(Op::CubicTo(b1.0, b1.1, b2.0, b2.1, b3.0, b3.1));
            i += 1;
        }
        if let Some(cp) = close_point {
            let ro = o.max_randomness_offset;
            ops.push(Op::LineTo(
                cp.0 + offset_opt(ro, o, 1.0),
                cp.1 + offset_opt(ro, o, 1.0),
            ));
        }
    } else if len == 3 {
        ops.push(Op::MoveTo(points[1].0, points[1].1));
        ops.push(Op::CubicTo(
            points[1].0,
            points[1].1,
            points[2].0,
            points[2].1,
            points[2].0,
            points[2].1,
        ));
    } else if len == 2 {
        ops.extend(double_line(
            points[0].0,
            points[0].1,
            points[1].0,
            points[1].1,
            o,
        ));
    }
    ops
}

fn curve_with_offset(points: &[(f64, f64)], offset: f64, o: &mut RoughOptions) -> Vec<Op> {
    let mut ps = Vec::with_capacity(points.len() + 3);
    let first = points[0];
    ps.push((
        first.0 + offset_opt(offset, o, 1.0),
        first.1 + offset_opt(offset, o, 1.0),
    ));
    ps.push((
        first.0 + offset_opt(offset, o, 1.0),
        first.1 + offset_opt(offset, o, 1.0),
    ));
    for (i, p) in points.iter().enumerate().skip(1) {
        ps.push((
            p.0 + offset_opt(offset, o, 1.0),
            p.1 + offset_opt(offset, o, 1.0),
        ));
        if i == points.len() - 1 {
            ps.push((
                p.0 + offset_opt(offset, o, 1.0),
                p.1 + offset_opt(offset, o, 1.0),
            ));
        }
    }
    curve_raw(&ps, None, o)
}

pub fn curve(points: &[(f64, f64)], o: &mut RoughOptions) -> Vec<Op> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut ops = curve_with_offset(points, 1.0 * (1.0 + o.roughness * 0.2), o);
    if !o.disable_multi_stroke {
        let mut overlay = o.second_pass();
        ops.extend(curve_with_offset(
            points,
            1.5 * (1.0 + o.roughness * 0.22),
            &mut overlay,
        ));
    }
    ops
}

#[allow(clippy::too_many_arguments)]
fn bezier_to(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x: f64,
    y: f64,
    current: (f64, f64),
    o: &mut RoughOptions,
) -> Vec<Op> {
    let ros = [o.max_randomness_offset, o.max_randomness_offset + 0.3];
    let iterations = if o.disable_multi_stroke { 1 } else { 2 };
    let mut ops = Vec::new();
    for i in 0..iterations {
        if i == 0 {
            ops.push(Op::MoveTo(current.0, current.1));
        } else {
            let (dx, dy) = if o.preserve_vertices {
                (0.0, 0.0)
            } else {
                (offset_opt(ros[0], o, 1.0), offset_opt(ros[0], o, 1.0))
            };
            ops.push(Op::MoveTo(current.0 + dx, current.1 + dy));
        }
        let (fx, fy) = if o.preserve_vertices {
            (x, y)
        } else {
            (
                x + offset_opt(ros[i], o, 1.0),
                y + offset_opt(ros[i], o, 1.0),
            )
        };
        ops.push(Op::CubicTo(
            x1 + offset_opt(ros[i], o, 1.0),
            y1 + offset_opt(ros[i], o, 1.0),
            x2 + offset_opt(ros[i], o, 1.0),
            y2 + offset_opt(ros[i], o, 1.0),
            fx,
            fy,
        ));
    }
    ops
}

#[derive(Debug, Clone, Copy)]
pub enum PathSeg {
    Move(f64, f64),
    Line(f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
}

pub fn svg_path(segments: &[PathSeg], o: &mut RoughOptions) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut current = (0.0, 0.0);
    for seg in segments {
        match *seg {
            PathSeg::Move(x, y) => current = (x, y),
            PathSeg::Line(x, y) => {
                ops.extend(double_line(current.0, current.1, x, y, o));
                current = (x, y);
            }
            PathSeg::Cubic(x1, y1, x2, y2, x, y) => {
                ops.extend(bezier_to(x1, y1, x2, y2, x, y, current, o));
                current = (x, y);
            }
        }
    }
    ops
}

fn ellipse_params(width: f64, height: f64, o: &mut RoughOptions) -> (f64, f64, f64) {
    let rx = (width / 2.0).abs();
    let ry = (height / 2.0).abs();
    let psq = (std::f64::consts::PI * 2.0 * ((rx.powi(2) + ry.powi(2)) / 2.0).sqrt()).sqrt();
    let step_count = (o
        .curve_step_count
        .max((o.curve_step_count / 200f64.sqrt()) * psq))
    .ceil();
    let increment = TAU / step_count;
    let curve_fit_randomness = 1.0 - o.curve_fitting;
    let mut rx = rx;
    let mut ry = ry;
    rx += offset_opt(rx * curve_fit_randomness, o, 1.0);
    ry += offset_opt(ry * curve_fit_randomness, o, 1.0);
    (increment, rx, ry)
}

#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    offset: f64,
    overlap: f64,
    o: &mut RoughOptions,
) -> (PointRing, PointRing) {
    let mut core_points = Vec::new();
    let mut all_points = Vec::new();
    if o.roughness == 0.0 {
        let increment = increment / 4.0;
        all_points.push((cx + rx * (-increment).cos(), cy + ry * (-increment).sin()));
        let mut angle = 0.0;
        while angle <= TAU {
            let p = (cx + rx * angle.cos(), cy + ry * angle.sin());
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        all_points.push((cx + rx, cy));
        all_points.push((cx + rx * increment.cos(), cy + ry * increment.sin()));
    } else {
        let rad_offset = offset_opt(0.5, o, 1.0) - FRAC_PI_2;
        all_points.push((
            offset_opt(offset, o, 1.0) + cx + 0.9 * rx * (rad_offset - increment).cos(),
            offset_opt(offset, o, 1.0) + cy + 0.9 * ry * (rad_offset - increment).sin(),
        ));
        let end_angle = TAU + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end_angle {
            let p = (
                offset_opt(offset, o, 1.0) + cx + rx * angle.cos(),
                offset_opt(offset, o, 1.0) + cy + ry * angle.sin(),
            );
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        let tail = rad_offset + TAU;
        all_points.push((
            offset_opt(offset, o, 1.0) + cx + rx * (tail + overlap * 0.5).cos(),
            offset_opt(offset, o, 1.0) + cy + ry * (tail + overlap * 0.5).sin(),
        ));
        all_points.push((
            offset_opt(offset, o, 1.0) + cx + 0.98 * rx * (rad_offset + overlap).cos(),
            offset_opt(offset, o, 1.0) + cy + 0.98 * ry * (rad_offset + overlap).sin(),
        ));
        all_points.push((
            offset_opt(offset, o, 1.0) + cx + 0.9 * rx * (rad_offset + overlap * 0.5).cos(),
            offset_opt(offset, o, 1.0) + cy + 0.9 * ry * (rad_offset + overlap * 0.5).sin(),
        ));
    }
    (all_points, core_points)
}

pub fn ellipse_ops(cx: f64, cy: f64, width: f64, height: f64, o: &mut RoughOptions) -> Vec<Op> {
    let (increment, rx, ry) = ellipse_params(width, height, o);

    let overlap = increment * offset(0.1, offset(0.4, 1.0, o, 1.0), o, 1.0);
    let (ap1, _) = compute_ellipse_points(increment, cx, cy, rx, ry, 1.0, overlap, o);
    let mut ops = curve_raw(&ap1, None, o);
    if !o.disable_multi_stroke && o.roughness != 0.0 {
        let (ap2, _) = compute_ellipse_points(increment, cx, cy, rx, ry, 1.5, 0.0, o);
        ops.extend(curve_raw(&ap2, None, o));
    }
    ops
}

pub struct RoughSeed(u64);

impl RoughSeed {
    pub fn new(seed: i64) -> Self {
        Self(seed as u64)
    }

    pub fn next_random(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x & 0xFFFF) as f64 / 65535.0
    }

    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_random() * (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shape::{DrawOptions, ops_to_polygon, rough_ellipse, rough_rectangle};

    fn opts(seed: i64, roughness: f64) -> DrawOptions {
        DrawOptions {
            seed,
            roughness,
            ..Default::default()
        }
    }

    fn perp_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= f64::EPSILON {
            return 0.0;
        }
        ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
    }

    #[test]
    fn identical_seeds_produce_identical_ops() {
        let o = opts(42, 1.0);
        let a = rough_rectangle(0.0, 0.0, 120.0, 80.0, &o);
        let b = rough_rectangle(0.0, 0.0, 120.0, 80.0, &o);
        assert_eq!(a.ops, b.ops);
        assert_eq!(a.sets, b.sets);

        let c = rough_ellipse(0.0, 0.0, 120.0, 80.0, &o);
        let d = rough_ellipse(0.0, 0.0, 120.0, 80.0, &o);
        assert_eq!(c.ops, d.ops);
    }

    #[test]
    fn rectangle_outline_is_bowed_by_cubics() {
        let d = rough_rectangle(0.0, 0.0, 100.0, 50.0, &opts(7, 1.0));
        let outline = &d.sets[0];
        assert!(
            outline.iter().any(|op| matches!(op, Op::CubicTo(..))),
            "a hand-drawn edge is a cubic, never a straight LineTo"
        );
        let moves = outline
            .iter()
            .filter(|op| matches!(op, Op::MoveTo(..)))
            .count();
        assert!(moves >= 2, "expected two overlay passes, got {moves}");

        let mut cur = (0.0, 0.0);
        let mut max_bow: f64 = 0.0;
        for op in outline {
            match *op {
                Op::MoveTo(x, y) | Op::LineTo(x, y) => cur = (x, y),
                Op::CubicTo(c1x, c1y, _, _, x, y) => {
                    max_bow = max_bow.max(perp_distance((c1x, c1y), cur, (x, y)));
                    cur = (x, y);
                }
                _ => {}
            }
        }
        assert!(
            max_bow > 0.2,
            "outline is not bowed, max deviation {max_bow}"
        );
    }

    #[test]
    fn zero_roughness_is_straight() {
        let d = rough_rectangle(0.0, 0.0, 100.0, 50.0, &opts(3, 0.0));
        let outline = &d.sets[0];
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        let mut cur = (0.0, 0.0);
        let mut max_bow: f64 = 0.0;
        for op in outline {
            match *op {
                Op::MoveTo(x, y) => {
                    cur = (x, y);
                    assert!(
                        corners
                            .iter()
                            .any(|c| (c.0 - x).abs() < 1e-9 && (c.1 - y).abs() < 1e-9),
                        "move ({x}, {y}) is not an exact corner"
                    );
                }
                Op::CubicTo(c1x, c1y, _, _, x, y) => {
                    max_bow = max_bow.max(perp_distance((c1x, c1y), cur, (x, y)));
                    cur = (x, y);
                    assert!(
                        corners
                            .iter()
                            .any(|c| (c.0 - x).abs() < 1e-9 && (c.1 - y).abs() < 1e-9),
                        "endpoint ({x}, {y}) is not an exact corner"
                    );
                }
                _ => {}
            }
        }
        assert!(max_bow < 1e-9, "roughness 0 must not bow, got {max_bow}");
    }

    #[test]
    fn ellipse_stays_near_the_ideal_and_has_two_passes() {
        let (w, h) = (120.0, 80.0);
        let d = rough_ellipse(0.0, 0.0, w, h, &opts(11, 1.0));
        let outline = &d.sets[0];
        let passes = outline
            .iter()
            .filter(|op| matches!(op, Op::MoveTo(..)))
            .count();
        assert!(passes >= 2, "expected two overlay passes, got {passes}");

        let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w / 2.0, h / 2.0);
        for p in ops_to_polygon(outline) {
            let v = (((p.x - cx) / rx).powi(2) + ((p.y - cy) / ry).powi(2)).sqrt();
            assert!(
                (v - 1.0).abs() < 0.2,
                "ellipse sample ({}, {}) deviates v={v}",
                p.x,
                p.y
            );
        }
    }
}
