use crate::core::{
    bounds::{Bounds, element_bounds},
    collision::distance_to_segment,
    element::{Element, ElementBase},
    geometry::Point,
    scene::Scene,
};

const EPSILON: f64 = 1e-6;
const ELLIPSE_SEGMENTS: usize = 24;

/// Ids of the elements fully enclosed by the lasso path, mirroring upstream's
/// default "contain" box selection mode: the whole outline has to sit inside
/// the polygon and the lasso itself must not cross the element.
pub fn elements_in_lasso(scene: &Scene, path: &[Point]) -> Vec<String> {
    if path.len() < 3 {
        return Vec::new();
    }
    let lasso_bounds = Bounds::from_points(path).expand(EPSILON);
    let mut ids = Vec::new();
    for element in scene.non_deleted() {
        if element.base().locked || is_bound_text(element) {
            continue;
        }
        let bounds = element_bounds(element);
        if !contains_bounds(&lasso_bounds, &bounds) {
            continue;
        }
        let outline = element_outline(element);
        let enclosed = if outline.len() < 2 {
            corners(&bounds)
                .iter()
                .all(|corner| polygon_contains(path, *corner))
        } else {
            outline.iter().all(|point| polygon_contains(path, *point))
        };
        if !enclosed || crosses_outline(path, &outline) {
            continue;
        }
        ids.push(element.id().to_string());
    }
    ids
}

fn is_bound_text(element: &Element) -> bool {
    matches!(element, Element::Text(text) if text.container_id.is_some())
}

fn contains_bounds(outer: &Bounds, inner: &Bounds) -> bool {
    outer.min_x <= inner.min_x
        && outer.min_y <= inner.min_y
        && outer.max_x >= inner.max_x
        && outer.max_y >= inner.max_y
}

fn corners(bounds: &Bounds) -> [Point; 4] {
    [
        Point::new(bounds.min_x, bounds.min_y),
        Point::new(bounds.max_x, bounds.min_y),
        Point::new(bounds.max_x, bounds.max_y),
        Point::new(bounds.min_x, bounds.max_y),
    ]
}

/// The element's outline in scene coordinates, with the first point repeated
/// at the end for closed shapes. Empty when the element has no outline of its
/// own, in which case its bounding box stands in for it.
fn element_outline(element: &Element) -> Vec<Point> {
    match element {
        Element::Line(line) => close(line.linear.points.clone(), line.polygon),
        Element::Arrow(arrow) => arrow.linear.points.clone(),
        Element::Freedraw(freedraw) => freedraw.points.clone(),
        Element::Ellipse { base } => ellipse_outline(base),
        Element::Diamond { base } => diamond_outline(base),
        Element::Rectangle { base } => rectangle_outline(base),
        Element::Text(text) => rectangle_outline(&text.base),
        Element::Image(image) => rectangle_outline(&image.base),
        Element::Frame(frame) | Element::MagicFrame(frame) => rectangle_outline(&frame.base),
        Element::Iframe(iframe) => rectangle_outline(&iframe.base),
        Element::Embeddable(embeddable) => rectangle_outline(&embeddable.base),
        Element::StickyNote(note) => rectangle_outline(&note.base),
        Element::Selection { .. } => Vec::new(),
    }
}

fn close(mut points: Vec<Point>, closed: bool) -> Vec<Point> {
    if closed && points.len() >= 2 {
        let first = points[0];
        points.push(first);
    }
    points
}

fn rotated(base: &ElementBase, point: Point) -> Point {
    if base.angle == 0.0 {
        return point;
    }
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    point.rotate(base.angle, center)
}

fn rectangle_outline(base: &ElementBase) -> Vec<Point> {
    let unrotated = [
        Point::new(base.x, base.y),
        Point::new(base.x + base.width, base.y),
        Point::new(base.x + base.width, base.y + base.height),
        Point::new(base.x, base.y + base.height),
    ];
    let mut points: Vec<Point> = unrotated
        .iter()
        .map(|point| rotated(base, *point))
        .collect();
    points.push(points[0]);
    points
}

fn diamond_outline(base: &ElementBase) -> Vec<Point> {
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let unrotated = [
        Point::new(base.x, center.y),
        Point::new(center.x, base.y),
        Point::new(base.x + base.width, center.y),
        Point::new(center.x, base.y + base.height),
    ];
    let mut points: Vec<Point> = unrotated
        .iter()
        .map(|point| rotated(base, *point))
        .collect();
    points.push(points[0]);
    points
}

fn ellipse_outline(base: &ElementBase) -> Vec<Point> {
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let rx = base.width / 2.0;
    let ry = base.height / 2.0;
    let mut points: Vec<Point> = (0..ELLIPSE_SEGMENTS)
        .map(|i| {
            let theta = i as f64 / ELLIPSE_SEGMENTS as f64 * std::f64::consts::TAU;
            rotated(
                base,
                Point::new(center.x + rx * theta.cos(), center.y + ry * theta.sin()),
            )
        })
        .collect();
    points.push(points[0]);
    points
}

fn polygon_contains(polygon: &[Point], p: Point) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[previous];
        if distance_to_segment(p, a, b) <= EPSILON {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            let x = a.x + (b.x - a.x) * t;
            if p.x < x {
                inside = !inside;
            }
        }
        previous = i;
    }
    inside
}

/// Whether any edge of the closed lasso path crosses one of the outline's
/// segments. Edges that merely touch an endpoint do not count.
fn crosses_outline(path: &[Point], outline: &[Point]) -> bool {
    if outline.len() < 2 {
        return false;
    }
    (0..path.len()).any(|i| {
        let a = path[i];
        let b = path[(i + 1) % path.len()];
        outline
            .windows(2)
            .any(|segment| segments_cross(a, b, segment[0], segment[1]))
    })
}

fn segments_cross(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1 = cross(a2 - a1, b1 - a1);
    let d2 = cross(a2 - a1, b2 - a1);
    let d3 = cross(b2 - b1, a1 - b1);
    let d4 = cross(b2 - b1, a2 - b1);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}

#[cfg(test)]
mod tests {
    use super::elements_in_lasso;
    use crate::core::{
        element::{Element, ElementBase, ElementType},
        geometry::Point,
        scene::Scene,
    };

    fn rect(scene: &mut Scene, id: &str, x: f64, y: f64, w: f64, h: f64) {
        let mut base = ElementBase::new(id.to_string(), ElementType::Rectangle);
        base.x = x;
        base.y = y;
        base.width = w;
        base.height = h;
        scene.add(Element::Rectangle { base });
    }

    fn lasso() -> Vec<Point> {
        [
            (-10.0, -10.0),
            (110.0, -10.0),
            (110.0, 110.0),
            (-10.0, 110.0),
        ]
        .iter()
        .map(|(x, y)| Point::new(*x, *y))
        .collect()
    }

    #[test]
    fn fully_enclosed_elements_are_selected() {
        let mut scene = Scene::new();
        rect(&mut scene, "inside", 0.0, 0.0, 100.0, 100.0);
        assert_eq!(elements_in_lasso(&scene, &lasso()), vec!["inside"]);
    }

    #[test]
    fn partially_covered_elements_are_not_selected() {
        let mut scene = Scene::new();
        rect(&mut scene, "crossing", 50.0, 50.0, 100.0, 100.0);
        assert!(elements_in_lasso(&scene, &lasso()).is_empty());
    }

    #[test]
    fn short_paths_select_nothing() {
        let mut scene = Scene::new();
        rect(&mut scene, "inside", 0.0, 0.0, 100.0, 100.0);
        let path = vec![Point::new(-10.0, -10.0), Point::new(110.0, 110.0)];
        assert!(elements_in_lasso(&scene, &path).is_empty());
    }
}
