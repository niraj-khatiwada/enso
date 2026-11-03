use crate::{
    bounds::element_bounds,
    element::Element,
    factory::new_id,
    geometry::Point,
    scene::Scene,
    transform::{scale_element, translate_element},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterH,
    Right,
    Top,
    CenterV,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribute {
    Horizontal,
    Vertical,
}

pub fn set_element_locked(scene: &mut Scene, id: &str, locked: bool) -> bool {
    match scene.get_mut(id) {
        Some(e) => {
            e.base_mut().locked = locked;
            true
        }
        None => false,
    }
}

pub fn toggle_element_locked(scene: &mut Scene, id: &str) -> bool {
    match scene.get_mut(id) {
        Some(e) => {
            let locked = !e.base().locked;
            e.base_mut().locked = locked;
            locked
        }
        None => false,
    }
}

pub fn group_ids(scene: &mut Scene, ids: &[String]) -> Option<String> {
    if ids.len() < 2 {
        return None;
    }
    let group_id = new_id();
    for id in ids {
        if let Some(e) = scene.get_mut(id) {
            let base = e.base_mut();
            if !base.group_ids.contains(&group_id) {
                base.group_ids.push(group_id.clone());
            }
        }
    }
    Some(group_id)
}

pub fn ungroup_ids(scene: &mut Scene, ids: &[String]) {
    for id in ids {
        if let Some(e) = scene.get_mut(id) {
            e.base_mut().group_ids.clear();
        }
    }
}

pub fn select_group(scene: &Scene, id: &str) -> Vec<String> {
    let Some(target) = scene.get(id) else {
        return Vec::new();
    };
    let groups: Vec<String> = target.base().group_ids.clone();
    if groups.is_empty() {
        return vec![id.to_string()];
    }
    scene
        .non_deleted()
        .filter(|e| e.base().group_ids.iter().any(|g| groups.contains(g)))
        .map(|e| e.id().to_string())
        .collect()
}

pub fn align_elements(scene: &mut Scene, ids: &[String], align: Align) {
    if ids.len() < 2 {
        return;
    }
    let elements: Vec<&Element> = ids
        .iter()
        .filter_map(|id| scene.get(id))
        .filter(|e| !e.base().locked)
        .collect();
    if elements.len() < 2 {
        return;
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for e in &elements {
        let b = element_bounds(e);
        min_x = min_x.min(b.min_x);
        max_x = max_x.max(b.max_x);
        min_y = min_y.min(b.min_y);
        max_y = max_y.max(b.max_y);
    }
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    let mut moves: Vec<(String, f64, f64)> = Vec::new();
    for e in elements {
        let b = element_bounds(e);
        let dx = match align {
            Align::Left => min_x - b.min_x,
            Align::CenterH => center_x - b.center().x,
            Align::Right => max_x - b.max_x,
            Align::Top => 0.0,
            Align::CenterV => 0.0,
            Align::Bottom => 0.0,
        };
        let dy = match align {
            Align::Top => min_y - b.min_y,
            Align::CenterV => center_y - b.center().y,
            Align::Bottom => max_y - b.max_y,
            Align::Left => 0.0,
            Align::CenterH => 0.0,
            Align::Right => 0.0,
        };
        if dx != 0.0 || dy != 0.0 {
            moves.push((e.id().to_string(), dx, dy));
        }
    }
    for (id, dx, dy) in moves {
        if let Some(el) = scene.get_mut(&id) {
            translate_element(el, dx, dy);
        }
    }
}

pub fn distribute_elements(scene: &mut Scene, ids: &[String], direction: Distribute) {
    if ids.len() < 3 {
        return;
    }
    let mut elements: Vec<&Element> = ids
        .iter()
        .filter_map(|id| scene.get(id))
        .filter(|e| !e.base().locked)
        .collect();
    if elements.len() < 3 {
        return;
    }

    match direction {
        Distribute::Horizontal => {
            elements.sort_by(|a, b| {
                element_bounds(a)
                    .center()
                    .x
                    .partial_cmp(&element_bounds(b).center().x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let first = element_bounds(elements[0]).center().x;
            let last = element_bounds(elements[elements.len() - 1]).center().x;
            let n = elements.len() as f64;
            let mut moves: Vec<(String, f64)> = Vec::new();
            for (i, e) in elements.iter().enumerate().skip(1).take(elements.len() - 2) {
                let target = first + (last - first) * (i as f64) / (n - 1.0);
                let current = element_bounds(e).center().x;
                moves.push((e.id().to_string(), target - current));
            }
            for (id, dx) in moves {
                if let Some(el) = scene.get_mut(&id) {
                    translate_element(el, dx, 0.0);
                }
            }
        }
        Distribute::Vertical => {
            elements.sort_by(|a, b| {
                element_bounds(a)
                    .center()
                    .y
                    .partial_cmp(&element_bounds(b).center().y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let first = element_bounds(elements[0]).center().y;
            let last = element_bounds(elements[elements.len() - 1]).center().y;
            let n = elements.len() as f64;
            let mut moves: Vec<(String, f64)> = Vec::new();
            for (i, e) in elements.iter().enumerate().skip(1).take(elements.len() - 2) {
                let target = first + (last - first) * (i as f64) / (n - 1.0);
                let current = element_bounds(e).center().y;
                moves.push((e.id().to_string(), target - current));
            }
            for (id, dy) in moves {
                if let Some(el) = scene.get_mut(&id) {
                    translate_element(el, 0.0, dy);
                }
            }
        }
    }
}

pub fn resize_selected(
    scene: &mut Scene,
    ids: &[String],
    scale_x: f64,
    scale_y: f64,
    pivot: Point,
) {
    for id in ids {
        if let Some(e) = scene.get_mut(id)
            && !e.base().locked
        {
            scale_element(e, scale_x, scale_y, pivot);
        }
    }
}

pub fn selected_bounds(scene: &Scene, ids: &[String]) -> Option<crate::bounds::Bounds> {
    let elements: Vec<&Element> = ids
        .iter()
        .filter_map(|id| scene.get(id))
        .filter(|e| !e.base().locked)
        .collect();
    crate::bounds::common_bounds(&elements)
}
