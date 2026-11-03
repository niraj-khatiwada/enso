use std::fmt::Write as _;

use crate::{
    core::{
        bounds::{Bounds, element_bounds},
        element::{Element, ElementType},
        geometry::Point,
        types::FillStyle,
    },
    render::{export::ops_to_path, shape::render_element},
};

/// Padding around the boundary geometry so strokes near the edge are not
/// clipped by the raster.
const MARGIN: f64 = 24.0;

/// Longest raster side, in pixels. Bigger scenes are rendered at a coarser
/// scale instead of allocating an unbounded bitmap.
const MAX_RASTER: f64 = 2048.0;

/// Upper bound for pixels per scene unit; small shapes stay crisp.
const MAX_SCALE: f64 = 2.0;

/// Largest raster dimension tiny-skia is asked to allocate.
const MAX_PIXELS_PER_SIDE: u32 = 4096;

/// Above this many boundary segments the planar arrangement is considered too
/// complex, mirroring upstream's `maxBoundarySegments`.
const MAX_BOUNDARY_SEGMENTS: usize = 2560;

/// Above this many generated polygon points the fill is rejected.
const MAX_POINTS: usize = 1536;

/// Smallest fillable area in scene units, mirroring upstream's `minArea`.
const MIN_AREA: f64 = 4.0;

/// Search radii tried in order, in the spirit of upstream's expanding
/// `fallbackSearchRadius` box. `INFINITY` means "every boundary element".
const SEARCH_RADII: [f64; 3] = [512.0, 2048.0, f64::INFINITY];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketFillError {
    /// No closed shape under the pointer and no enclosed region found; upstream
    /// stays silent for this one.
    NoOwner,
    /// An owner exists but the region leaks out of it.
    OpenRegion,
    TooComplex,
    TooSmall,
    InvalidPolygon,
}

#[derive(Debug, Clone)]
pub struct BucketFillRegion {
    pub points: Vec<Point>,
    /// Enclosing element the fill should be inserted above, when there is one.
    pub owner_id: Option<String>,
    /// Elements that bounded the region, used as the insertion anchor when the
    /// region is formed by open lines rather than a single owner.
    pub boundary_ids: Vec<String>,
}

/// Elements that render a visible outline and can therefore bound a region.
fn is_boundary(element: &Element) -> bool {
    let base = element.base();
    if base.is_deleted || base.opacity <= 0.0 {
        return false;
    }
    if crate::core::color::is_transparent(&base.stroke_color) {
        return false;
    }
    matches!(
        element.kind(),
        ElementType::Rectangle
            | ElementType::Diamond
            | ElementType::Ellipse
            | ElementType::Frame
            | ElementType::MagicFrame
            | ElementType::Line
            | ElementType::Freedraw
    )
}

/// Whether the element is paint the bucket tool could have produced: a closed
/// line polygon with a background and no visible stroke.
pub fn is_fill_compatible(element: &Element) -> bool {
    if let Element::Line(line) = element {
        let base = &line.linear.base;
        line.polygon
            && line.linear.points.len() >= 3
            && base.opacity > 0.0
            && !crate::core::color::is_transparent(&base.background_color)
            && crate::core::color::is_transparent(&base.stroke_color)
    } else {
        false
    }
}

fn line_ring(element: &Element) -> Option<&[Point]> {
    match element {
        Element::Line(line) => Some(&line.linear.points),
        _ => None,
    }
}

/// Even-odd point-in-polygon test over a scene-space ring.
pub fn point_in_ring(ring: &[Point], point: Point) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[j];
        if (a.y > point.y) != (b.y > point.y) {
            let t = (point.y - a.y) / (b.y - a.y);
            let x = a.x + (b.x - a.x) * t;
            if point.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Index of the topmost existing fill whose paint covers `point`.
pub fn find_fill_at(elements: &[Element], point: Point) -> Option<usize> {
    elements
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, element)| {
            let ring = line_ring(element)?;
            (is_fill_compatible(element) && point_in_ring(ring, point)).then_some(index)
        })
}

/// Closed shapes that a click can land inside; upstream's
/// `isClosedOwnerCandidate`. Frames and plain shapes are always candidates,
/// lines only once they are polygons, freedraw only when it loops back.
fn is_owner_candidate(element: &Element) -> bool {
    let base = element.base();
    if base.is_deleted
        || base.opacity <= 0.0
        || is_fill_compatible(element)
        || crate::core::color::is_transparent(&base.stroke_color)
    {
        return false;
    }
    match element {
        Element::Line(line) => line.polygon && line.linear.points.len() >= 3,
        Element::Freedraw(freedraw) => is_path_a_loop(&freedraw.points),
        _ => matches!(
            element.kind(),
            ElementType::Rectangle
                | ElementType::Diamond
                | ElementType::Ellipse
                | ElementType::Frame
                | ElementType::MagicFrame
        ),
    }
}

fn is_path_a_loop(points: &[Point]) -> bool {
    if points.len() < 8 {
        return false;
    }
    let span = Bounds::from_points(points);
    let tolerance = (span.width().max(span.height()) * 0.25).max(10.0);
    points
        .first()
        .zip(points.last())
        .is_some_and(|(first, last)| first.distance(*last) <= tolerance)
}

fn point_in_element(element: &Element, point: Point) -> bool {
    let base = element.base();
    let (x, y, w, h) = (base.x, base.y, base.width, base.height);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    match element.kind() {
        ElementType::Ellipse => {
            let dx = (point.x - cx) / (w / 2.0);
            let dy = (point.y - cy) / (h / 2.0);
            dx * dx + dy * dy <= 1.0
        }
        ElementType::Diamond => {
            (point.x - cx).abs() / (w / 2.0) + (point.y - cy).abs() / (h / 2.0) <= 1.0
        }
        ElementType::Line | ElementType::Freedraw => {
            line_ring(element).is_some_and(|ring| point_in_ring(ring, point))
        }
        _ => element_bounds(element).contains(point),
    }
}

fn find_owner(point: Point, elements: &[Element]) -> Option<&Element> {
    elements
        .iter()
        .rev()
        .find(|element| is_owner_candidate(element) && point_in_element(element, point))
}

/// Segment count used for the complexity guard.
fn segment_count(element: &Element) -> usize {
    match element {
        Element::Line(line) => line.linear.points.len(),
        Element::Freedraw(freedraw) => freedraw.points.len(),
        _ => 8,
    }
}

/// Computes the fillable region under `point` by rasterizing the visible
/// outlines, flooding the enclosed area and tracing the resulting contour.
pub fn compute_bucket_fill(
    point: Point,
    elements: &[Element],
) -> Result<BucketFillRegion, BucketFillError> {
    let boundaries: Vec<&Element> = elements.iter().filter(|e| is_boundary(e)).collect();
    if boundaries.is_empty() {
        return Err(BucketFillError::NoOwner);
    }
    let segments: usize = boundaries.iter().map(|e| segment_count(e)).sum();
    if segments > MAX_BOUNDARY_SEGMENTS {
        return Err(BucketFillError::TooComplex);
    }

    let owner_id = find_owner(point, elements).map(|element| element.id().to_string());

    for radius in SEARCH_RADII {
        let search = if radius.is_finite() {
            Bounds::new(
                point.x - radius,
                point.y - radius,
                point.x + radius,
                point.y + radius,
            )
        } else {
            let mut union = Bounds::new(point.x, point.y, point.x, point.y);
            for element in &boundaries {
                union = union.union(&element_bounds(element));
            }
            union
        };

        let candidates: Vec<&Element> = if radius.is_finite() {
            boundaries
                .iter()
                .copied()
                .filter(|e| element_bounds(e).intersects(&search))
                .collect()
        } else {
            boundaries.clone()
        };
        if candidates.is_empty() {
            continue;
        }

        let mut bounds = Bounds::new(point.x, point.y, point.x, point.y);
        for element in &candidates {
            bounds = bounds.union(&element_bounds(element));
        }
        if radius.is_finite() {
            bounds = Bounds::new(
                bounds.min_x.max(search.min_x),
                bounds.min_y.max(search.min_y),
                bounds.max_x.min(search.max_x),
                bounds.max_y.min(search.max_y),
            );
        }
        bounds = bounds.expand(MARGIN);
        if bounds.width().max(bounds.height()) > 250_000.0 {
            return Err(BucketFillError::TooComplex);
        }

        let Some((pixmap, scale)) = rasterize(&candidates, bounds, point) else {
            continue;
        };
        let Some(region) = flood_region(&pixmap, point, bounds, scale) else {
            continue;
        };
        let area = region.iter().filter(|filled| **filled).count() as f64 / (scale * scale);
        if area < MIN_AREA {
            return Err(BucketFillError::TooSmall);
        }
        let Some(contour) =
            trace_contour(&region, pixmap.width() as usize, pixmap.height() as usize)
        else {
            return Err(BucketFillError::InvalidPolygon);
        };
        let mut points: Vec<Point> = contour
            .into_iter()
            .map(|(px, py)| {
                Point::new(
                    bounds.min_x + (px as f64 + 0.5) / scale,
                    bounds.min_y + (py as f64 + 0.5) / scale,
                )
            })
            .collect();
        points = simplify_closed(&points, 1.2 / scale);
        dedupe(&mut points);
        if points.len() < 3 {
            return Err(BucketFillError::InvalidPolygon);
        }
        if points.len() > MAX_POINTS {
            return Err(BucketFillError::TooComplex);
        }
        let boundary_ids = candidates
            .iter()
            .map(|element| element.id().to_string())
            .collect();
        return Ok(BucketFillRegion {
            points,
            owner_id,
            boundary_ids,
        });
    }

    Err(if owner_id.is_some() {
        BucketFillError::OpenRegion
    } else {
        BucketFillError::NoOwner
    })
}

/// Rasterizes the outlines of `elements` into `bounds`, at most `MAX_RASTER`
/// pixels on the long side. Returns the bitmap and its pixels-per-unit scale.
fn rasterize(
    elements: &[&Element],
    bounds: Bounds,
    point: Point,
) -> Option<(tiny_skia::Pixmap, f64)> {
    let width = bounds.width().max(1.0);
    let height = bounds.height().max(1.0);
    let scale = (MAX_RASTER / width.max(height)).min(MAX_SCALE);
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let pixel_width = (width * scale).ceil().max(1.0) as u32;
    let pixel_height = (height * scale).ceil().max(1.0) as u32;
    if pixel_width > MAX_PIXELS_PER_SIDE || pixel_height > MAX_PIXELS_PER_SIDE {
        return None;
    }
    if point.x < bounds.min_x
        || point.x > bounds.max_x
        || point.y < bounds.min_y
        || point.y > bounds.max_y
    {
        return None;
    }

    let svg = boundary_svg(elements, bounds);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(pixel_width, pixel_height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    Some((pixmap, scale))
}

fn boundary_svg(elements: &[&Element], bounds: Bounds) -> String {
    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.2}\" height=\"{:.2}\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\">",
        bounds.width(),
        bounds.height(),
        bounds.min_x,
        bounds.min_y,
        bounds.width(),
        bounds.height()
    );
    for element in elements {
        let mut outline = (*element).clone();
        {
            let base = outline.base_mut();
            base.background_color = "transparent".to_string();
            base.fill_style = FillStyle::Solid;
            base.stroke_color = "#000000".to_string();
            base.opacity = 100.0;
        }
        for drawable in render_element(&outline) {
            if let Some(ink) = &drawable.ink {
                let _ = write!(
                    svg,
                    "<path d=\"{}\" fill=\"#000000\" stroke=\"none\"/>",
                    ops_to_path(ink)
                );
                continue;
            }
            for set in &drawable.sets {
                let _ = write!(
                    svg,
                    "<path d=\"{}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                    ops_to_path(set),
                    drawable.options.stroke_width.max(0.5)
                );
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Floods the pixels reachable from `point` that are not painted by a stroke.
/// Returns `None` when the region reaches the raster border (the region is
/// open, so no closed fill can be derived from it).
fn flood_region(
    pixmap: &tiny_skia::Pixmap,
    point: Point,
    bounds: Bounds,
    scale: f64,
) -> Option<Vec<bool>> {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let data = pixmap.data();
    let free = |x: usize, y: usize| data[(y * width + x) * 4 + 3] == 0;

    let start_x = ((point.x - bounds.min_x) * scale).floor() as i64;
    let start_y = ((point.y - bounds.min_y) * scale).floor() as i64;
    let start = find_free_pixel(&free, start_x, start_y, width, height)?;

    let mut region = vec![false; width * height];
    let mut stack: Vec<(usize, usize)> = vec![start];
    region[start.1 * width + start.0] = true;
    while let Some((x, y)) = stack.pop() {
        if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
            return None;
        }
        let neighbors = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];
        for (nx, ny) in neighbors {
            if !region[ny * width + nx] && free(nx, ny) {
                region[ny * width + nx] = true;
                stack.push((nx, ny));
            }
        }
    }
    Some(region)
}

/// Finds a free pixel around `(x, y)`, widening the search ring by ring so a
/// click that lands exactly on a stroke still starts inside the region.
fn find_free_pixel(
    free: &impl Fn(usize, usize) -> bool,
    x: i64,
    y: i64,
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    let inside =
        |px: i64, py: i64| px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height;
    if inside(x, y) && free(x as usize, y as usize) {
        return Some((x as usize, y as usize));
    }
    for radius in 1..=8i64 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (px, py) = (x + dx, y + dy);
                if inside(px, py) && free(px as usize, py as usize) {
                    return Some((px as usize, py as usize));
                }
            }
        }
    }
    None
}

/// Moore-neighbour tracing of the region's outer boundary, returned as pixel
/// coordinates.
fn trace_contour(region: &[bool], width: usize, height: usize) -> Option<Vec<(i32, i32)>> {
    let start = (0..height).find_map(|y| {
        (0..width)
            .find(|x| region[y * width + x])
            .map(|x| (x as i32, y as i32))
    })?;

    // Clockwise 8-neighbourhood, starting east.
    const OFFSETS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let in_region = |x: i32, y: i32| {
        x >= 0
            && y >= 0
            && (x as usize) < width
            && (y as usize) < height
            && region[y as usize * width + x as usize]
    };

    let mut contour = vec![start];
    let mut current = start;
    let mut backtrack = (start.0 - 1, start.1);
    let max_steps = 4 * width * height + 16;
    for _ in 0..max_steps {
        let dir = OFFSETS
            .iter()
            .position(|offset| (current.0 + offset.0, current.1 + offset.1) == backtrack)
            .unwrap_or(0);
        let mut found = None;
        for step in 1..=8 {
            let index = (dir + step) % 8;
            let candidate = (current.0 + OFFSETS[index].0, current.1 + OFFSETS[index].1);
            if in_region(candidate.0, candidate.1) {
                found = Some((index, candidate));
                break;
            }
        }
        let Some((index, next)) = found else {
            break;
        };
        let previous = (index + 7) % 8;
        backtrack = (
            current.0 + OFFSETS[previous].0,
            current.1 + OFFSETS[previous].1,
        );
        current = next;
        if current == start && contour.len() > 1 {
            break;
        }
        contour.push(current);
    }
    if contour.len() < 3 {
        None
    } else {
        Some(contour)
    }
}

/// Ramer-Douglas-Peucker for a closed ring: the ring is split at the vertex
/// farthest from its first point, so both halves are simplified as open chains.
fn simplify_closed(points: &[Point], epsilon: f64) -> Vec<Point> {
    if points.len() <= 3 {
        return points.to_vec();
    }
    let anchor = 0;
    let far = (1..points.len())
        .max_by(|a, b| {
            let da = squared_distance(points[anchor], points[*a]);
            let db = squared_distance(points[anchor], points[*b]);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1);
    let first: Vec<Point> = points[..=far].to_vec();
    let mut second: Vec<Point> = points[far..].to_vec();
    second.push(points[0]);

    let mut simplified = simplify_open(&first, epsilon);
    let mut tail = simplify_open(&second, epsilon);
    simplified.pop();
    tail.pop();
    simplified.extend(tail);
    simplified
}

fn simplify_open(points: &[Point], epsilon: f64) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let start = points[0];
    let end = *points.last().unwrap();
    let mut index = 0;
    let mut max_distance = 0.0;
    for (i, point) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let distance = distance_to_segment(*point, start, end);
        if distance > max_distance {
            max_distance = distance;
            index = i;
        }
    }
    if max_distance > epsilon {
        let mut left = simplify_open(&points[..=index], epsilon);
        let right = simplify_open(&points[index..], epsilon);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![start, end]
    }
}

fn dedupe(points: &mut Vec<Point>) {
    points.dedup_by(|a, b| a.distance(*b) < 0.01);
    if points.len() > 2
        && let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
        && first.distance(last) < 0.01
    {
        points.pop();
    }
}

fn squared_distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn distance_to_segment(point: Point, start: Point, end: Point) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return point.distance(start);
    }
    let t =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    point.distance(Point::new(start.x + dx * t, start.y + dy * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Element {
        crate::core::factory::new_rectangle(
            x,
            y,
            w,
            h,
            &crate::core::factory::ElementOptions::default(),
        )
    }

    #[test]
    fn an_orphan_click_finds_no_owner() {
        assert!(matches!(
            compute_bucket_fill(Point::new(10.0, 10.0), &[]),
            Err(BucketFillError::NoOwner)
        ));
    }

    #[test]
    fn a_click_inside_a_rectangle_yields_a_closed_fill() {
        let elements = [rectangle(0.0, 0.0, 200.0, 120.0)];
        let region =
            compute_bucket_fill(Point::new(100.0, 60.0), &elements).expect("fillable region");
        assert!(region.points.len() >= 4);
        let bounds = Bounds::from_points(&region.points);
        assert!(bounds.min_x > -2.0 && bounds.max_x < 202.0);
        assert!(bounds.min_y > -2.0 && bounds.max_y < 122.0);
        assert!(bounds.width() > 190.0 && bounds.height() > 110.0);
    }

    #[test]
    fn a_click_outside_every_shape_is_open() {
        let elements = [rectangle(0.0, 0.0, 200.0, 120.0)];
        assert!(matches!(
            compute_bucket_fill(Point::new(1000.0, 1000.0), &elements),
            Err(BucketFillError::NoOwner)
        ));
    }

    #[test]
    fn a_generated_fill_is_recognised_as_paint() {
        let options = crate::core::factory::ElementOptions {
            stroke_color: "transparent".to_string(),
            background_color: "#ffc9c9".to_string(),
            fill_style: FillStyle::Solid,
            stroke_width: 1.0,
            stroke_style: crate::core::types::StrokeStyle::Solid,
            roughness: 0.0,
            opacity: 100.0,
        };
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 40.0),
            Point::new(0.0, 40.0),
        ];
        let mut fill = crate::core::factory::new_line(points, &options);
        if let Element::Line(line) = &mut fill {
            line.polygon = true;
        }
        let elements = [fill];
        assert!(is_fill_compatible(&elements[0]));
        assert_eq!(find_fill_at(&elements, Point::new(20.0, 20.0)), Some(0));
        assert_eq!(find_fill_at(&elements, Point::new(80.0, 20.0)), None);
        let drawables = render_element(&elements[0]);
        assert!(
            drawables
                .iter()
                .any(|drawable| drawable.fill_path.is_some()),
            "a polygon fill must expose a closed fill path for the canvas and exports"
        );
    }
}
