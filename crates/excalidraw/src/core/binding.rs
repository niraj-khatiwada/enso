use crate::{
    bounds::{Bounds, element_bounds},
    collision::hit_test_element,
    element::{BindMode, Element, ElementType, FixedPointBinding},
    geometry::Point,
    scene::Scene,
};

pub const BINDING_THRESHOLD: f64 = 18.0;

pub fn focus_point(element: &Element, from: Point) -> Point {
    let base = element.base();
    let w = base.width.max(0.0001);
    let h = base.height.max(0.0001);
    let center = Point::new(base.x + w / 2.0, base.y + h / 2.0);

    let local_from = from.rotate(-base.angle, center);
    let dx = local_from.x - center.x;
    let dy = local_from.y - center.y;

    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return Point::new(center.x + w / 2.0, center.y).rotate(base.angle, center);
    }

    let hw = w / 2.0;
    let hh = h / 2.0;

    let local = match element.kind() {
        ElementType::Diamond => {
            let nx = dx.abs() / hw;
            let ny = dy.abs() / hh;
            let sum = nx + ny;
            let s = if sum > 1e-9 { 1.0 / sum } else { 0.0 };
            Point::new(center.x + dx * s, center.y + dy * s)
        }
        ElementType::Ellipse => {
            let denom = ((dx / hw).powi(2) + (dy / hh).powi(2)).sqrt();
            let t = if denom > 1e-9 { 1.0 / denom } else { 0.0 };
            Point::new(center.x + dx * t, center.y + dy * t)
        }
        _ => {
            let tx = if dx.abs() > 1e-9 {
                hw / dx.abs()
            } else {
                f64::INFINITY
            };
            let ty = if dy.abs() > 1e-9 {
                hh / dy.abs()
            } else {
                f64::INFINITY
            };
            let t = tx.min(ty);
            if !t.is_finite() {
                Point::new(center.x + hw, center.y)
            } else {
                Point::new(center.x + dx * t, center.y + dy * t)
            }
        }
    };

    local.rotate(base.angle, center)
}

pub fn find_bindable(
    scene: &Scene,
    point: Point,
    threshold: f64,
    exclude: &[String],
) -> Option<String> {
    scene
        .non_deleted()
        .filter(|e| e.is_bindable() && !e.base().locked)
        .filter(|e| !exclude.iter().any(|id| id == e.id()))
        .find(|e| {
            hit_test_element(e, point, threshold)
                || element_bounds(e).expand(threshold).contains(point)
        })
        .map(|e| e.id().to_string())
}

pub fn make_binding(element: &Element, from: Point) -> FixedPointBinding {
    let focus = focus_point(element, from);
    let b: Bounds = element_bounds(element);
    let fx = if b.width() > 0.0 {
        (focus.x - b.min_x) / b.width()
    } else {
        0.5
    };
    let fy = if b.height() > 0.0 {
        (focus.y - b.min_y) / b.height()
    } else {
        0.5
    };
    FixedPointBinding {
        element_id: element.id().to_string(),
        fixed_point: [fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0)],
        mode: BindMode::Inside,
    }
}

struct BindJob {
    id: String,
    start: Option<String>,
    end: Option<String>,
    points: Vec<Point>,
}

pub fn update_bindings(scene: &mut Scene) {
    let jobs: Vec<BindJob> = scene
        .non_deleted()
        .filter_map(|e| match e {
            Element::Arrow(a) => Some(BindJob {
                id: e.id().to_string(),
                start: a
                    .linear
                    .start_binding
                    .as_ref()
                    .map(|b| b.element_id.clone()),
                end: a.linear.end_binding.as_ref().map(|b| b.element_id.clone()),
                points: a.linear.points.clone(),
            }),
            Element::Line(l) => Some(BindJob {
                id: e.id().to_string(),
                start: l
                    .linear
                    .start_binding
                    .as_ref()
                    .map(|b| b.element_id.clone()),
                end: l.linear.end_binding.as_ref().map(|b| b.element_id.clone()),
                points: l.linear.points.clone(),
            }),
            _ => None,
        })
        .filter(|j| j.start.is_some() || j.end.is_some())
        .collect();

    for job in jobs {
        if job.points.len() < 2 {
            continue;
        }
        let n = job.points.len();
        let mut pts = job.points.clone();
        if let Some(sid) = &job.start
            && let Some(target) = scene.get(sid)
        {
            pts[0] = focus_point(target, pts[1]);
        }
        if let Some(eid) = &job.end
            && let Some(target) = scene.get(eid)
        {
            pts[n - 1] = focus_point(target, pts[n - 2]);
        }
        let b = Bounds::from_points(&pts);
        if let Some(el) = scene.get_mut(&job.id) {
            match el {
                Element::Arrow(a) => a.linear.points = pts,
                Element::Line(l) => l.linear.points = pts,
                _ => continue,
            }
            let base = el.base_mut();
            base.x = b.min_x;
            base.y = b.min_y;
            base.width = b.width();
            base.height = b.height();
        }
    }
}

pub fn snap_to_grid(v: f64, grid: f64) -> f64 {
    if grid <= 0.0 {
        v
    } else {
        (v / grid).round() * grid
    }
}

pub fn sync_container_text(scene: &mut Scene) {
    let pad = 8.0;
    let jobs: Vec<(String, String, f64, f64, String)> = scene
        .non_deleted()
        .filter_map(|e| {
            if let Element::Text(t) = e {
                let cid = t.container_id.as_ref()?;
                Some((
                    t.base.id.clone(),
                    cid.clone(),
                    t.font_size,
                    t.line_height,
                    t.text.clone(),
                ))
            } else {
                None
            }
        })
        .collect();

    for (_, cid, font_size, line_height, text) in &jobs {
        let Some(bounds) = scene.get(cid).map(element_bounds) else {
            continue;
        };
        let inner_w = (bounds.width() - pad * 2.0).max(*font_size);
        let lines = crate::text::wrap_text(text, inner_w, *font_size)
            .split('\n')
            .count()
            .max(1);
        let text_h = lines as f64 * font_size * line_height;
        if let Some(Element::StickyNote(note)) = scene.get_mut(cid) {
            note.base.height = (text_h + pad * 2.0).max(note.base_height);
        }
    }

    for (id, cid, font_size, line_height, text) in jobs {
        if let Some(points) = scene.get(&cid).and_then(linear_points) {
            // Arrow labels sit at the path midpoint like upstream's bound text
            // and keep their intrinsic size instead of filling the container.
            let Some(mid) = path_midpoint(&points) else {
                continue;
            };
            let width = crate::text::text_width(&text, font_size);
            let height = crate::text::text_height(&text, font_size, line_height);
            if let Some(Element::Text(t)) = scene.get_mut(&id) {
                t.base.x = mid.x - width / 2.0;
                t.base.y = mid.y - height / 2.0;
                t.base.width = width;
                t.base.height = height;
                t.text_align = crate::types::TextAlign::Center;
                t.vertical_align = crate::types::VerticalAlign::Middle;
            }
            continue;
        }

        let Some(bounds) = scene.get(&cid).map(element_bounds) else {
            continue;
        };
        let inner_w = (bounds.width() - pad * 2.0).max(font_size);
        let lines = crate::text::wrap_text(&text, inner_w, font_size)
            .split('\n')
            .count()
            .max(1);
        let text_h = lines as f64 * font_size * line_height;
        let x = bounds.min_x + pad;
        let y = bounds.min_y + ((bounds.height() - text_h) / 2.0).max(pad * 0.5);
        if let Some(Element::Text(t)) = scene.get_mut(&id) {
            t.base.x = x;
            t.base.y = y;
            t.base.width = inner_w;
            t.base.height = text_h;
            t.text_align = crate::types::TextAlign::Center;
            t.vertical_align = crate::types::VerticalAlign::Middle;
        }
    }
}

fn linear_points(element: &Element) -> Option<Vec<Point>> {
    match element {
        Element::Arrow(a) => Some(a.linear.points.clone()),
        Element::Line(l) => Some(l.linear.points.clone()),
        _ => None,
    }
}

/// Point half way along a polyline, matching upstream's
/// `getPointAtPathParameter(element, 0.5)` used to place arrow labels.
fn path_midpoint(points: &[Point]) -> Option<Point> {
    if points.len() < 2 {
        return points.first().copied();
    }
    let total: f64 = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    if total <= 1e-9 {
        return points.first().copied();
    }
    let target = total / 2.0;
    let mut walked = 0.0;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = a.distance(b);
        if walked + len >= target {
            let t = if len > 1e-9 {
                (target - walked) / len
            } else {
                0.0
            };
            return Some(a.lerp(b, t));
        }
        walked += len;
    }
    points.last().copied()
}

pub fn refresh_derived(scene: &mut Scene) {
    update_bindings(scene);
    sync_container_text(scene);
}

pub fn constrain_angle(origin: Point, p: Point, step_deg: f64) -> Point {
    let d = p - origin;
    let len = d.length();
    if len < 1e-9 {
        return p;
    }
    let step = step_deg.to_radians();
    let angle = d.y.atan2(d.x);
    let snapped = (angle / step).round() * step;
    origin + Point::new(snapped.cos(), snapped.sin()) * len
}

pub fn constrain_square(start: Point, current: Point) -> Point {
    let dx = current.x - start.x;
    let dy = current.y - start.y;
    let size = dx.abs().max(dy.abs());
    Point::new(
        start.x + size * if dx < 0.0 { -1.0 } else { 1.0 },
        start.y + size * if dy < 0.0 { -1.0 } else { 1.0 },
    )
}
