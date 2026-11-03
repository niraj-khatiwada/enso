use crate::{element::Element, geometry::Point};

pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

pub fn hit_test_rectangle(element: &Element, p: Point) -> bool {
    let base = element.base();
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let local = p.rotate(-base.angle, center);
    let x = base.x;
    let y = base.y;
    local.x >= x && local.x <= x + base.width && local.y >= y && local.y <= y + base.height
}

pub fn hit_test_ellipse(element: &Element, p: Point) -> bool {
    let base = element.base();
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let local = p.rotate(-base.angle, center);
    let rx = base.width / 2.0;
    let ry = base.height / 2.0;
    if rx <= 0.0 || ry <= 0.0 {
        return false;
    }
    let nx = (local.x - center.x) / rx;
    let ny = (local.y - center.y) / ry;
    nx * nx + ny * ny <= 1.0
}

pub fn hit_test_diamond(element: &Element, p: Point) -> bool {
    let base = element.base();
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let local = p.rotate(-base.angle, center);
    let nx = (local.x - center.x).abs() / (base.width / 2.0);
    let ny = (local.y - center.y).abs() / (base.height / 2.0);
    nx + ny <= 1.0
}

pub fn hit_test_linear(element: &Element, p: Point, threshold: f64) -> bool {
    let points = match element {
        Element::Line(l) => &l.linear.points,
        Element::Arrow(a) => &a.linear.points,
        _ => return false,
    };
    points
        .windows(2)
        .any(|w| distance_to_segment(p, w[0], w[1]) <= threshold)
}

/// Closed line polygons are hit anywhere inside, not only near their outline,
/// which is what makes bucket fills selectable after painting.
pub fn hit_test_polygon(element: &Element, p: Point) -> bool {
    let Element::Line(line) = element else {
        return false;
    };
    line.polygon && line.linear.points.len() >= 3 && point_in_ring(&line.linear.points, p)
}

fn point_in_ring(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[j];
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            let x = a.x + (b.x - a.x) * t;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

pub fn hit_test_freedraw(element: &Element, p: Point, threshold: f64) -> bool {
    if let Element::Freedraw(f) = element {
        f.points
            .windows(2)
            .any(|w| distance_to_segment(p, w[0], w[1]) <= threshold)
    } else {
        false
    }
}

pub fn hit_test_element(element: &Element, p: Point, threshold: f64) -> bool {
    match element.kind() {
        crate::element::ElementType::Ellipse => hit_test_ellipse(element, p),
        crate::element::ElementType::Diamond => hit_test_diamond(element, p),
        crate::element::ElementType::Rectangle
        | crate::element::ElementType::Text
        | crate::element::ElementType::Image
        | crate::element::ElementType::Frame
        | crate::element::ElementType::MagicFrame
        | crate::element::ElementType::Embeddable
        | crate::element::ElementType::Iframe
        | crate::element::ElementType::StickyNote => hit_test_rectangle(element, p),
        crate::element::ElementType::Line | crate::element::ElementType::Arrow => {
            hit_test_polygon(element, p) || hit_test_linear(element, p, threshold)
        }
        crate::element::ElementType::Freedraw => hit_test_freedraw(element, p, threshold),
        crate::element::ElementType::Selection => false,
    }
}

pub fn hit_test_point_in_bounds(element: &Element, p: Point) -> bool {
    let base = element.base();
    p.x >= base.x && p.x <= base.x + base.width && p.y >= base.y && p.y <= base.y + base.height
}
