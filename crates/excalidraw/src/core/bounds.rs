use serde::{Deserialize, Serialize};

use crate::{element::Element, geometry::Point};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_points(points: &[Point]) -> Self {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    pub fn center(&self) -> Point {
        Point::new(
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        )
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }

    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds::new(
            self.min_x.min(other.min_x),
            self.min_y.min(other.min_y),
            self.max_x.max(other.max_x),
            self.max_y.max(other.max_y),
        )
    }

    pub fn expand(&self, delta: f64) -> Bounds {
        Bounds::new(
            self.min_x - delta,
            self.min_y - delta,
            self.max_x + delta,
            self.max_y + delta,
        )
    }
}

pub fn element_bounds(element: &Element) -> Bounds {
    let base = element.base();
    let x = base.x;
    let y = base.y;
    let w = base.width;
    let h = base.height;
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let center = Point::new(x + half_w, y + half_h);

    let corners = [
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ];

    if base.angle != 0.0 {
        let rotated: Vec<Point> = corners
            .iter()
            .map(|c| c.rotate(base.angle, center))
            .collect();
        Bounds::from_points(&rotated)
    } else {
        Bounds::new(x, y, x + w, y + h)
    }
}

pub fn common_bounds(elements: &[&Element]) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    for e in elements {
        let b = element_bounds(e);
        bounds = Some(match bounds {
            None => b,
            Some(prev) => prev.union(&b),
        });
    }
    bounds
}
