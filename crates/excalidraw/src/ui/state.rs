use std::collections::{HashMap, HashSet};

use crate::core::{
    element::{ConvertTarget, Element, ElementType},
    geometry::Point,
    scene::{BinaryFileData, Scene},
    transform::translate_element,
    types::{FillStyle, StrokeStyle, Theme},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Selection,
    Hand,
    Rectangle,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    FreeDraw,
    Text,
    Image,
    Frame,
    Eraser,
    BucketFill,
    Lasso,
    Laser,
    StickyNote,
    Embeddable,
}

impl Tool {
    pub fn to_element_type(self) -> Option<ElementType> {
        match self {
            Self::Rectangle => Some(ElementType::Rectangle),
            Self::Diamond => Some(ElementType::Diamond),
            Self::Ellipse => Some(ElementType::Ellipse),
            Self::Arrow => Some(ElementType::Arrow),
            Self::Line => Some(ElementType::Line),
            Self::FreeDraw => Some(ElementType::Freedraw),
            Self::Frame => Some(ElementType::Frame),
            Self::StickyNote => Some(ElementType::StickyNote),
            Self::Embeddable => Some(ElementType::Embeddable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Rotate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragMode {
    Move { last: Point },
    Resize { handle: ResizeHandle, pivot: Point },
    Rotate { center: Point, last: Point },

    Endpoint { index: usize },
}

pub fn is_bound_text(element: &Element) -> bool {
    matches!(element, Element::Text(t) if t.container_id.is_some())
}

fn repoint_label(copy: &mut Element, remap: &HashMap<String, String>) {
    if let Element::Text(t) = copy
        && let Some(container) = t.container_id.clone()
        && let Some(new_container) = remap.get(&container)
    {
        t.container_id = Some(new_container.clone());
    }
    if let Some(bound) = copy.base_mut().bound_elements.as_mut() {
        for item in bound.iter_mut() {
            if let Some(new_id) = remap.get(&item.id) {
                item.id = new_id.clone();
            }
        }
    }
}

pub fn selection_bounds(
    scene: &Scene,
    selected: &HashSet<String>,
) -> Option<crate::core::bounds::Bounds> {
    let elements: Vec<&Element> = scene
        .non_deleted()
        .filter(|e| selected.contains(e.id()))
        .collect();
    crate::core::bounds::common_bounds(&elements)
}

#[derive(Debug, Clone)]
pub struct ElementStyle {
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roughness: f64,
    pub opacity: f64,
    pub font_size: f64,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".to_string(),
            background_color: "transparent".to_string(),
            fill_style: FillStyle::Solid,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roughness: 1.0,
            opacity: 100.0,
            font_size: 20.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub scene: Scene,
    pub theme: Theme,

    pub view_background_color: String,
    pub style: ElementStyle,
    pub selected: HashSet<String>,
    pub zoom: f64,
    pub scroll: Point,
    pub tool: Tool,
    pub show_grid: bool,
    pub snap_enabled: bool,
    pub grid_size: f64,
    pub library: Vec<Element>,
    pub show_library: bool,
    pub show_layers: bool,
    pub clipboard: Vec<Element>,

    pub tool_locked: bool,

    pub select_wrap: bool,

    pub input_trackpad: bool,

    pub arrow_binding: bool,

    pub snap_to_objects: bool,

    pub snap_to_midpoints: bool,

    pub zen_mode: bool,

    pub view_mode: bool,

    pub show_stats: bool,
    pub files: HashMap<String, BinaryFileData>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            scene: Scene::new(),
            theme: Theme::Light,
            style: ElementStyle::default(),
            selected: HashSet::new(),
            zoom: 1.0,
            scroll: Point::zero(),
            tool: Tool::Selection,
            view_background_color: "#ffffff".to_string(),

            show_grid: false,
            snap_enabled: false,
            grid_size: 20.0,
            library: Vec::new(),
            show_library: false,
            show_layers: false,
            clipboard: Vec::new(),
            tool_locked: false,
            select_wrap: true,
            input_trackpad: true,
            arrow_binding: true,
            snap_to_objects: false,
            snap_to_midpoints: true,
            zen_mode: false,
            view_mode: false,
            show_stats: false,
            files: HashMap::new(),
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn select(&mut self, id: &str) {
        self.selected.clear();
        self.selected.insert(id.to_string());
    }

    pub fn select_many(&mut self, ids: impl IntoIterator<Item = String>) {
        self.selected.clear();
        self.selected.extend(ids);
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn selected_elements(&self) -> Vec<&Element> {
        self.scene
            .non_deleted()
            .filter(|e| self.selected.contains(e.id()))
            .collect()
    }

    pub fn element_at(&self, point: Point, threshold: f64) -> Option<String> {
        self.hit_element(point, threshold, false)
    }

    pub fn text_target_at(&self, point: Point, threshold: f64) -> Option<String> {
        self.hit_element(point, threshold, true)
    }

    fn hit_element(
        &self,
        point: Point,
        threshold: f64,
        include_bound_text: bool,
    ) -> Option<String> {
        let elements: Vec<&Element> = self
            .scene
            .non_deleted()
            .filter(|e| !e.base().locked)
            .filter(|e| include_bound_text || !is_bound_text(e))
            .collect();
        elements
            .iter()
            .rev()
            .find(|e| crate::core::collision::hit_test_element(e, point, threshold))
            .map(|e| e.id().to_string())
    }

    pub fn container_text_id(&self, container_id: &str) -> Option<String> {
        self.scene
            .non_deleted()
            .find(|e| {
                matches!(e, Element::Text(t) if t.container_id.as_deref() == Some(container_id))
            })
            .map(|e| e.id().to_string())
    }

    pub fn elements_at_including_locked(&self, point: Point, threshold: f64) -> Vec<&Element> {
        self.scene
            .non_deleted()
            .filter(|e| !is_bound_text(e))
            .filter(|e| crate::core::collision::hit_test_element(e, point, threshold))
            .collect()
    }

    pub fn locked_target(&self, id: &str) -> Vec<&Element> {
        match self.scene.get(id) {
            Some(element) if !element.is_deleted() => vec![element],
            _ => self
                .scene
                .non_deleted()
                .filter(|e| e.base().group_ids.iter().any(|group| group == id))
                .collect(),
        }
    }

    pub fn move_selected(&mut self, dx: f64, dy: f64) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id)
                && !e.base().locked
            {
                translate_element(e, dx, dy);
            }
        }
        self.refresh_bindings();
    }

    pub fn resize_selected(&mut self, scale_x: f64, scale_y: f64, pivot: Point) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        crate::core::operations::resize_selected(&mut self.scene, &ids, scale_x, scale_y, pivot);

        for id in &ids {
            if let Some(Element::StickyNote(note)) = self.scene.get_mut(id) {
                note.base_height = note.base.height;
            }
        }
        self.refresh_bindings();
    }

    pub fn rotate_selected(&mut self, angle: f64, pivot: Point) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id)
                && !e.base().locked
            {
                crate::core::transform::rotate_element(e, angle, pivot);
            }
        }
        self.refresh_bindings();
    }

    pub fn delete_selected(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in &ids {
            self.scene.remove(id);
        }

        let orphans: Vec<String> = self
            .scene
            .elements
            .iter()
            .filter(|e| match e {
                Element::Text(t) => t
                    .container_id
                    .as_ref()
                    .map(|c| ids.contains(c))
                    .unwrap_or(false),
                _ => false,
            })
            .map(|e| e.id().to_string())
            .collect();
        for id in orphans {
            self.scene.remove(&id);
        }
        self.selected.clear();
    }

    pub fn duplicate_selected(&mut self) {
        let top_level: Vec<String> = self.selected.iter().cloned().collect();
        let ids = self.selected_with_labels();

        let remap: HashMap<String, String> = ids
            .iter()
            .map(|id| (id.clone(), crate::core::factory::new_id()))
            .collect();
        for id in &ids {
            if let Some(e) = self.scene.get(id) {
                let mut copy = e.clone();
                copy.base_mut().id = remap[id].clone();
                repoint_label(&mut copy, &remap);
                translate_element(&mut copy, 20.0, 20.0);
                self.scene.add(copy);
            }
        }
        self.selected.clear();
        self.selected
            .extend(top_level.iter().map(|id| remap[id].clone()));
    }

    pub fn group_selected(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        if let Some(_gid) = crate::core::operations::group_ids(&mut self.scene, &ids) {}
    }

    pub fn ungroup_selected(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        crate::core::operations::ungroup_ids(&mut self.scene, &ids);
    }

    pub fn select_group_members(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        let mut all = HashSet::new();
        for id in &ids {
            all.extend(crate::core::operations::select_group(&self.scene, id));
        }
        self.selected = all;
    }

    pub fn align_selected(&mut self, align: crate::core::operations::Align) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        crate::core::operations::align_elements(&mut self.scene, &ids, align);
    }

    pub fn distribute_selected(&mut self, dir: crate::core::operations::Distribute) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        crate::core::operations::distribute_elements(&mut self.scene, &ids, dir);
    }

    pub fn set_selected_locked(&mut self, locked: bool) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().locked = locked;
            }
        }
    }

    pub fn set_selected_stroke_color(&mut self, color: &str) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().stroke_color = color.to_string();
            }
        }
    }

    pub fn set_selected_background_color(&mut self, color: &str) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().background_color = color.to_string();
            }
        }
    }

    pub fn set_selected_fill_style(&mut self, style: FillStyle) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().fill_style = style;
            }
        }
    }

    pub fn set_selected_stroke_width(&mut self, width: f64) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().stroke_width = width;
            }
        }
    }

    pub fn set_selected_stroke_style(&mut self, style: StrokeStyle) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().stroke_style = style;
            }
        }
    }

    pub fn set_selected_opacity(&mut self, opacity: f64) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().opacity = opacity;
            }
        }
    }

    pub fn set_selected_roughness(&mut self, roughness: f64) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().roughness = roughness;
            }
        }
    }

    pub fn set_selected_font_size(&mut self, size: f64) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(Element::Text(t)) = self.scene.get_mut(&id) {
                t.font_size = size;
            }
        }
    }

    pub fn set_selected_text(&mut self, text: &str) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(Element::Text(t)) = self.scene.get_mut(&id) {
                t.text = text.to_string();
            }
        }
    }

    pub fn set_selected_font_family(&mut self, family: crate::core::types::FontFamily) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(Element::Text(t)) = self.scene.get_mut(&id) {
                t.font_family = family;
            }
        }
    }

    pub fn set_selected_text_align(&mut self, align: crate::core::types::TextAlign) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(Element::Text(t)) = self.scene.get_mut(&id) {
                t.text_align = align;
            }
        }
    }

    pub fn set_selected_link(&mut self, link: Option<&str>) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().link = link.map(str::to_string);
            }
        }
    }

    pub fn set_selected_roundness(&mut self, roundness: Option<crate::core::types::Roundness>) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.scene.get_mut(&id) {
                e.base_mut().roundness = roundness;
            }
        }
    }

    pub fn convert_selected_to(&mut self, target: ConvertTarget) -> bool {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        let mut converted = false;
        for id in ids {
            let Some(element) = self.scene.get_mut(&id) else {
                continue;
            };
            if let Some(next) = element.convert_to(target) {
                *element = next;
                converted = true;
            }
        }
        converted
    }

    pub fn lone_selected_element(&self) -> Option<&Element> {
        if self.selected.len() != 1 {
            return None;
        }
        self.selected_elements().into_iter().next()
    }

    pub fn add_selected_to_library(&mut self) {
        let elements: Vec<Element> = self.selected_elements().into_iter().cloned().collect();
        for e in elements {
            if !self.library.iter().any(|l| l.id() == e.id()) {
                self.library.push(e);
            }
        }
    }

    pub fn insert_from_library(&mut self, library_id: &str) -> Option<String> {
        let element = self.library.iter().find(|e| e.id() == library_id)?.clone();
        let mut copy = element;
        let new_id = crate::core::factory::new_id();
        copy.base_mut().id = new_id.clone();
        copy.base_mut().x += 40.0;
        copy.base_mut().y += 40.0;
        self.scene.add(copy);
        Some(new_id)
    }

    pub fn remove_from_library(&mut self, library_id: &str) {
        self.library.retain(|e| e.id() != library_id);
    }

    pub fn layer_order(&self) -> Vec<String> {
        self.scene
            .non_deleted()
            .map(|e| e.id().to_string())
            .collect()
    }

    pub fn bring_forward(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            if let Some(pos) = self.scene.elements.iter().position(|e| e.id() == id)
                && pos + 1 < self.scene.elements.len()
            {
                self.scene.elements.swap(pos, pos + 1);
            }
        }
    }

    pub fn send_backward(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids.iter().rev() {
            if let Some(pos) = self.scene.elements.iter().position(|e| e.id() == id)
                && pos > 0
            {
                self.scene.elements.swap(pos, pos - 1);
            }
        }
    }

    pub fn bring_to_front(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        let mut extracted: Vec<Element> = Vec::new();
        self.scene.elements.retain(|e| {
            if ids.contains(&e.id().to_string()) {
                extracted.push(e.clone());
                false
            } else {
                true
            }
        });
        self.scene.elements.extend(extracted);
    }

    pub fn send_to_back(&mut self) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        let mut extracted: Vec<Element> = Vec::new();
        self.scene.elements.retain(|e| {
            if ids.contains(&e.id().to_string()) {
                extracted.push(e.clone());
                false
            } else {
                true
            }
        });
        let rest = self.scene.elements.clone();
        self.scene.elements.clear();
        self.scene.elements.extend(extracted);
        self.scene.elements.extend(rest);
    }

    pub fn select_all(&mut self) {
        let ids: Vec<String> = self
            .scene
            .non_deleted()
            .filter(|e| !e.base().locked && !is_bound_text(e))
            .map(|e| e.id().to_string())
            .collect();
        self.selected.clear();
        self.selected.extend(ids);
    }

    pub fn selected_with_labels(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in self.selected.iter() {
            if let Some(text_id) = self.container_text_id(id)
                && !ids.contains(&text_id)
            {
                ids.push(text_id);
            }
        }
        ids
    }

    pub fn copy_selected(&mut self) {
        let ids = self.selected_with_labels();
        self.clipboard = self
            .scene
            .non_deleted()
            .filter(|e| ids.contains(&e.id().to_string()))
            .cloned()
            .collect();
    }

    pub fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    pub fn has_clipboard(&self) -> bool {
        !self.clipboard.is_empty()
    }

    pub fn paste(&mut self, dx: f64, dy: f64) {
        if self.clipboard.is_empty() {
            return;
        }

        let remap: HashMap<String, String> = self
            .clipboard
            .iter()
            .map(|src| (src.id().to_string(), crate::core::factory::new_id()))
            .collect();
        let mut new_ids = Vec::new();
        for src in self.clipboard.clone() {
            let mut copy = src;
            let new_id = remap[copy.id()].clone();
            let is_label = is_bound_text(&copy);
            copy.base_mut().id = new_id.clone();
            copy.base_mut().is_deleted = false;
            repoint_label(&mut copy, &remap);
            translate_element(&mut copy, dx, dy);

            if !is_label {
                new_ids.push(new_id);
            }
            self.scene.add(copy);
        }
        self.selected.clear();
        self.selected.extend(new_ids);

        for e in self.clipboard.iter_mut() {
            translate_element(e, dx, dy);
        }
    }

    pub fn snap(&self, p: Point) -> Point {
        if self.snap_enabled {
            Point::new(
                crate::core::binding::snap_to_grid(p.x, self.grid_size),
                crate::core::binding::snap_to_grid(p.y, self.grid_size),
            )
        } else {
            p
        }
    }

    pub fn refresh_bindings(&mut self) {
        crate::core::binding::refresh_derived(&mut self.scene);
    }

    pub fn selected_linear_points(&self) -> Option<(String, Vec<Point>)> {
        if self.selected.len() != 1 {
            return None;
        }
        let id = self.selected.iter().next()?.clone();
        let points = match self.scene.get(&id)? {
            Element::Line(l) => l.linear.points.clone(),
            Element::Arrow(a) => a.linear.points.clone(),
            _ => return None,
        };
        Some((id, points))
    }

    pub fn move_linear_endpoint(&mut self, id: &str, index: usize, p: Point) {
        let points = match self.scene.get(id) {
            Some(Element::Line(l)) => l.linear.points.clone(),
            Some(Element::Arrow(a)) => a.linear.points.clone(),
            _ => return,
        };
        let n = points.len();
        if n < 2 || index >= n {
            return;
        }
        let mut pts = points;
        pts[index] = p;
        if let Some(Element::Line(l)) = self.scene.get_mut(id) {
            l.linear.points = pts.clone();
            let b = crate::core::bounds::Bounds::from_points(&pts);
            l.linear.base.x = b.min_x;
            l.linear.base.y = b.min_y;
            l.linear.base.width = b.width();
            l.linear.base.height = b.height();
        } else if let Some(Element::Arrow(a)) = self.scene.get_mut(id) {
            a.linear.points = pts.clone();
            let b = crate::core::bounds::Bounds::from_points(&pts);
            a.linear.base.x = b.min_x;
            a.linear.base.y = b.min_y;
            a.linear.base.width = b.width();
            a.linear.base.height = b.height();
        }

        let is_start = index == 0;
        let other = if is_start { pts[1] } else { pts[n - 2] };
        let exclude = vec![id.to_string()];
        let target = crate::core::binding::find_bindable(
            &self.scene,
            p,
            crate::core::binding::BINDING_THRESHOLD / self.zoom,
            &exclude,
        );
        self.attach_linear_binding(id, is_start, target.as_deref(), other);
    }

    pub fn attach_linear_binding(
        &mut self,
        id: &str,
        start: bool,
        target: Option<&str>,
        other: Point,
    ) {
        let binding = target.and_then(|tid| {
            self.scene
                .get(tid)
                .map(|el| crate::core::binding::make_binding(el, other))
        });
        match self.scene.get_mut(id) {
            Some(Element::Line(l)) => {
                if start {
                    l.linear.start_binding = binding
                } else {
                    l.linear.end_binding = binding
                }
            }
            Some(Element::Arrow(a)) => {
                if start {
                    a.linear.start_binding = binding
                } else {
                    a.linear.end_binding = binding
                }
            }
            _ => {}
        }
        self.refresh_bindings();
    }

    pub fn set_selected_arrowhead(
        &mut self,
        end: bool,
        head: Option<crate::core::arrowhead::Arrowhead>,
    ) {
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        for id in ids {
            match self.scene.get_mut(&id) {
                Some(Element::Line(l)) => {
                    if end {
                        l.linear.end_arrowhead = head;
                    } else {
                        l.linear.start_arrowhead = head;
                    }
                }
                Some(Element::Arrow(a)) => {
                    if end {
                        a.linear.end_arrowhead = head;
                    } else {
                        a.linear.start_arrowhead = head;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn zoom_to_fit(&mut self, viewport: (f64, f64), padding: f64) {
        let elements: Vec<&Element> = self.scene.non_deleted().collect();
        let Some(b) = crate::core::bounds::common_bounds(&elements) else {
            self.zoom = 1.0;
            self.scroll = Point::zero();
            return;
        };
        self.frame_bounds(&b, viewport, padding);
    }

    pub fn zoom_to_selection(&mut self, viewport: (f64, f64), padding: f64) {
        let Some(b) = selection_bounds(&self.scene, &self.selected) else {
            self.zoom_to_fit(viewport, padding);
            return;
        };
        self.frame_bounds(&b, viewport, padding);
    }

    fn frame_bounds(
        &mut self,
        b: &crate::core::bounds::Bounds,
        viewport: (f64, f64),
        padding: f64,
    ) {
        let (vw, vh) = viewport;
        let w = b.width().max(1.0);
        let h = b.height().max(1.0);
        let zoom = ((vw - padding * 2.0) / w).min((vh - padding * 2.0) / h);
        self.zoom = zoom.clamp(0.05, 8.0);
        let c = b.center();
        self.scroll = Point::new(vw / 2.0 - c.x * self.zoom, vh / 2.0 - c.y * self.zoom);
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    scene: Vec<Element>,
}

#[derive(Debug, Clone, Default)]
pub struct History {
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    max_depth: usize,
}

impl History {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    pub fn push(&mut self, scene: &Scene) {
        self.undo_stack.push(Snapshot {
            scene: scene.elements.clone(),
        });
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.redo_stack.push(Snapshot {
                scene: scene.elements.clone(),
            });
            scene.elements = snapshot.scene;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(Snapshot {
                scene: scene.elements.clone(),
            });
            scene.elements = snapshot.scene;
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

pub fn screen_to_world(screen: Point, zoom: f64, scroll: Point) -> Point {
    Point::new((screen.x - scroll.x) / zoom, (screen.y - scroll.y) / zoom)
}

pub fn world_to_screen(world: Point, zoom: f64, scroll: Point) -> Point {
    Point::new(world.x * zoom + scroll.x, world.y * zoom + scroll.y)
}
