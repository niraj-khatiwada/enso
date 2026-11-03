use crate::{element::Element, geometry::Point};

pub fn translate_element(element: &mut Element, dx: f64, dy: f64) {
    let base = element.base_mut();
    base.x += dx;
    base.y += dy;
    match element {
        Element::Line(l) => {
            for p in l.linear.points.iter_mut() {
                p.x += dx;
                p.y += dy;
            }
        }
        Element::Arrow(a) => {
            for p in a.linear.points.iter_mut() {
                p.x += dx;
                p.y += dy;
            }
            if let Some(segs) = a.fixed_segments.as_mut() {
                for s in segs.iter_mut() {
                    s.start.x += dx;
                    s.start.y += dy;
                    s.end.x += dx;
                    s.end.y += dy;
                }
            }
        }
        Element::Freedraw(f) => {
            for p in f.points.iter_mut() {
                p.x += dx;
                p.y += dy;
            }
        }
        _ => {}
    }
}

pub fn rotate_element(element: &mut Element, angle: f64, pivot: Point) {
    let base = element.base_mut();
    let center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);
    let new_center = center.rotate(angle, pivot);
    base.angle += angle;
    base.x = new_center.x - base.width / 2.0;
    base.y = new_center.y - base.height / 2.0;

    let old_center = center;
    match element {
        Element::Line(l) => {
            for p in l.linear.points.iter_mut() {
                *p = p.rotate(angle, old_center);
            }
        }
        Element::Arrow(a) => {
            for p in a.linear.points.iter_mut() {
                *p = p.rotate(angle, old_center);
            }
            if let Some(segs) = a.fixed_segments.as_mut() {
                for s in segs.iter_mut() {
                    s.start = s.start.rotate(angle, old_center);
                    s.end = s.end.rotate(angle, old_center);
                }
            }
        }
        Element::Freedraw(f) => {
            for p in f.points.iter_mut() {
                *p = p.rotate(angle, old_center);
            }
        }
        _ => {}
    }
}

pub fn scale_element(element: &mut Element, scale_x: f64, scale_y: f64, pivot: Point) {
    let base = element.base_mut();
    let old_center = Point::new(base.x + base.width / 2.0, base.y + base.height / 2.0);

    base.width = (base.width * scale_x).abs();
    base.height = (base.height * scale_y).abs();
    base.x = pivot.x + (base.x - pivot.x) * scale_x;
    base.y = pivot.y + (base.y - pivot.y) * scale_y;

    if base.width < 0.0 {
        base.x += base.width;
        base.width = -base.width;
    }
    if base.height < 0.0 {
        base.y += base.height;
        base.height = -base.height;
    }

    match element {
        Element::Line(l) => {
            for p in l.linear.points.iter_mut() {
                p.x = pivot.x + (p.x - pivot.x) * scale_x;
                p.y = pivot.y + (p.y - pivot.y) * scale_y;
            }
        }
        Element::Arrow(a) => {
            for p in a.linear.points.iter_mut() {
                p.x = pivot.x + (p.x - pivot.x) * scale_x;
                p.y = pivot.y + (p.y - pivot.y) * scale_y;
            }
        }
        Element::Freedraw(f) => {
            for p in f.points.iter_mut() {
                p.x = pivot.x + (p.x - pivot.x) * scale_x;
                p.y = pivot.y + (p.y - pivot.y) * scale_y;
            }
        }
        _ => {}
    }

    let _ = old_center;
}
