use std::{collections::HashMap, sync::Arc};

use gpui_kit::{base::StyledExt, prelude::FluentBuilder as _, *};

use crate::{
    chrome,
    core::{
        arrowhead::Arrowhead,
        bounds::Bounds,
        element::{ConvertTarget, Element},
        factory::*,
        geometry::Point,
        scene::{AppState, Scene},
        types::{FontFamily, StrokeStyle, Theme},
    },
    design::{Tokens, size},
    i18n::{I18n, Language},
    render::bucket_fill::BucketFillError,
    state::{Document, DragMode, History, ResizeHandle, Tool, screen_to_world, selection_bounds},
};

const CLICK_DEFAULT_SIZE: f64 = 100.0;

const CLICK_THRESHOLD: f64 = 3.0;

const PASTE_LINE_GAP: f64 = 10.0;

/// Lifetime of a laser trail point, matching upstream's `DECAY_TIME`.
const LASER_LIFETIME: std::time::Duration = std::time::Duration::from_millis(1000);

/// Repaint cadence while a laser trail decays.
const LASER_FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// One sample of the laser pointer trail, aged so the oldest points fade out
/// first instead of the whole trail disappearing at once.
#[derive(Clone, Copy)]
struct LaserPoint {
    point: Point,
    born: std::time::Instant,
}

fn laser_trail_at(points: &[LaserPoint], now: std::time::Instant) -> Vec<(Point, f64)> {
    let lifetime = LASER_LIFETIME.as_secs_f64();
    points
        .iter()
        .filter_map(|sample| {
            let age = now.saturating_duration_since(sample.born).as_secs_f64();
            let remaining = 1.0 - age / lifetime;
            (remaining > 0.0).then_some((sample.point, remaining.min(1.0)))
        })
        .collect()
}

pub(crate) type Handler<E> = Box<dyn Fn(&E, &mut Window, &mut App)>;

pub struct Editor {
    /// The empty canvas's welcome screen (Excalidraw.com's, which talks about
    /// browser storage and sign-up). Hosts that save to files turn it off.
    pub welcome: bool,
    pub document: Document,
    pub history: History,
    pub i18n: I18n,
    drawing: bool,
    drag_start: Option<Point>,
    last_point: Option<Point>,
    current_points: Vec<Point>,
    panning: bool,
    pan_start: Option<Point>,
    pan_scroll: Option<Point>,
    drag_mode: Option<DragMode>,
    resize_origin: Option<Bounds>,
    rotate_start_angle: f64,
    editing_text: Option<String>,
    text_edit_started: bool,

    replace_on_type: bool,
    marquee_start: Option<Point>,
    marquee_current: Option<Point>,
    /// Points traced by the lasso tool while the pointer is down.
    lasso_path: Vec<Point>,
    lasso_keep_selection: bool,
    /// Selection kept when the lasso started with Shift held.
    lasso_base_selection: Vec<String>,
    laser_points: Vec<LaserPoint>,
    laser_active: bool,
    laser_fading: bool,
    erasing: bool,

    pending_points: Vec<Point>,

    /// Scene point armed by a bucket-fill pointer down, committed on pointer up.
    pending_bucket_fill: Option<Point>,

    cursor_world: Option<Point>,

    cursor_mods: Modifiers,

    cursor_screen: (f64, f64),

    focus_handle: Option<FocusHandle>,

    drag_target: Option<String>,

    drag_start_world: Option<Point>,

    drag_anchor: Option<Point>,

    drag_accum: Point,
    space_pan: bool,

    context_menu: Option<Point>,

    active_locked_id: Option<String>,

    show_menu: bool,

    show_more_tools: bool,

    show_prefs: bool,

    show_lang: bool,

    status: Option<String>,

    file_drag: Option<FileDragState>,

    cursor_hint: Option<CursorHintState>,

    toast: Option<ToastState>,

    eye_dropper: Option<EyeDropperState>,

    overlay_nonce: u64,

    cursor_hint_shown_at: Option<std::time::Instant>,

    show_palette: bool,
    palette_query: String,
    palette_index: usize,
    /// Id of the last executed command, surfaced as upstream's "Recents" row.
    palette_last_used: Option<&'static str>,

    show_find: bool,
    find_query: String,
    find_matches: Vec<String>,
    find_index: usize,

    show_help: bool,

    viewport: (f64, f64),
    image_sources: HashMap<String, Arc<RenderImage>>,
    pending_image_pick: bool,

    pending_scene_open: bool,

    show_clear_dialog: bool,
    show_save_dialog: bool,
    pending_save_dialog: bool,
    project_name: String,
    /// Caret position inside `project_name`, as a byte offset kept on a `char`
    /// boundary. The selection is the span between this and `name_anchor`.
    name_caret: usize,
    /// The fixed end of the selection; equal to `name_caret` when nothing is
    /// selected. Tracking both ends is what lets shift+arrow grow a selection.
    name_anchor: usize,

    pending_scene_save: bool,

    show_image_dialog: bool,
    pending_image_dialog: bool,
    /// Set by the palette's copy-as-PNG/SVG rows; the render pass performs the
    /// clipboard write, which needs `App`.
    pending_copy_png: bool,
    pending_copy_svg: bool,
    export_scale: f32,
    export_with_background: bool,
    export_dark_mode: bool,
    export_embed_scene: bool,
    export_only_selected: bool,
    /// Rasterized snapshot of the scene shown in the export dialog preview.
    export_preview: Option<Arc<RenderImage>>,

    show_mermaid_dialog: bool,
    mermaid_tab: MermaidTab,
    mermaid_text: String,
    /// Caret position inside `mermaid_text`, kept on a `char` boundary.
    mermaid_caret: usize,
    mermaid_anchor: usize,
    /// Elements rebuilt from `mermaid_text` and drawn in the preview pane.
    mermaid_preview: Vec<Element>,
    mermaid_error: Option<String>,

    color_popup: Option<ColorTarget>,

    /// The inner span of the properties panel's `changeOpacity` row, recorded
    /// while painting. Pointer positions are mapped onto this span to turn a
    /// click or drag into a 0..100 value.
    opacity_track_bounds: Option<gpui_kit::Bounds<Pixels>>,

    shade_stroke: usize,
    shade_background: usize,

    hex_buffer: String,
    hex_focused: bool,

    font_popup: bool,
    font_query: String,

    show_link_dialog: bool,
    /// Whether the link popup shows its input (`editor`) or the read-only
    /// summary with the pencil button (`info`), mirroring upstream's
    /// `showHyperlinkPopup`.
    link_editing: bool,
    link_input: String,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            welcome: true,
            document: Document::new(),
            history: History::new(100),
            i18n: I18n::new(Language::detect()),
            drawing: false,
            drag_start: None,
            last_point: None,
            current_points: Vec::new(),
            panning: false,
            pan_start: None,
            pan_scroll: None,
            drag_mode: None,
            resize_origin: None,
            rotate_start_angle: 0.0,
            editing_text: None,
            text_edit_started: false,
            replace_on_type: false,
            marquee_start: None,
            marquee_current: None,
            lasso_path: Vec::new(),
            lasso_keep_selection: false,
            lasso_base_selection: Vec::new(),
            laser_points: Vec::new(),
            laser_active: false,
            laser_fading: false,
            erasing: false,
            pending_points: Vec::new(),
            pending_bucket_fill: None,
            cursor_world: None,
            cursor_mods: Modifiers::default(),
            cursor_screen: (0.0, 0.0),
            focus_handle: None,
            drag_target: None,
            drag_start_world: None,
            drag_anchor: None,
            drag_accum: Point::zero(),
            space_pan: false,
            context_menu: None,
            active_locked_id: None,
            show_menu: false,
            show_more_tools: false,
            show_prefs: false,
            show_lang: false,
            status: None,
            file_drag: None,
            cursor_hint: None,
            toast: None,
            eye_dropper: None,
            overlay_nonce: 0,
            cursor_hint_shown_at: None,
            show_palette: false,
            palette_query: String::new(),
            palette_index: 0,
            palette_last_used: None,
            show_find: false,
            find_query: String::new(),
            find_matches: Vec::new(),
            find_index: 0,
            show_help: false,
            viewport: (800.0, 600.0),
            image_sources: HashMap::new(),
            pending_image_pick: false,
            pending_scene_open: false,
            show_clear_dialog: false,
            show_save_dialog: false,
            pending_save_dialog: false,
            project_name: String::new(),
            name_caret: 0,
            name_anchor: 0,
            pending_scene_save: false,
            show_image_dialog: false,
            pending_image_dialog: false,
            pending_copy_png: false,
            pending_copy_svg: false,
            export_scale: 1.0,
            export_with_background: true,
            export_dark_mode: false,
            export_embed_scene: false,
            export_only_selected: false,
            export_preview: None,
            show_mermaid_dialog: false,
            mermaid_tab: MermaidTab::Mermaid,
            mermaid_text: String::new(),
            mermaid_caret: 0,
            mermaid_anchor: 0,
            mermaid_preview: Vec::new(),
            mermaid_error: None,
            color_popup: None,
            opacity_track_bounds: None,
            shade_stroke: crate::theme::DEFAULT_STROKE_SHADE,
            shade_background: crate::theme::DEFAULT_BACKGROUND_SHADE,
            hex_buffer: String::new(),
            hex_focused: false,
            font_popup: false,
            font_query: String::new(),
            show_link_dialog: false,
            link_editing: false,
            link_input: String::new(),
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.cancel_pending();
        self.active_locked_id = None;
        self.document.tool = tool;
        if tool == Tool::Image {
            self.pending_image_pick = true;
        }
        self.drawing = false;
        self.drag_start = None;
        self.current_points.clear();
        self.drag_mode = None;
        self.marquee_start = None;
        self.lasso_path.clear();
        self.lasso_base_selection.clear();
        self.lasso_keep_selection = false;
        self.laser_points.clear();
        self.laser_active = false;
        self.pending_bucket_fill = None;
        if tool == Tool::BucketFill {
            // The bucket panel is the background picker; a lingering selection
            // would show the element style panel instead.
            self.document.clear_selection();
        }
        if tool != Tool::Text {
            self.commit_text_edit();
        }
    }

    pub fn set_language(&mut self, language: Language) {
        self.i18n.set_language(language);
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.document.set_theme(theme);
    }

    /// Replaces the whole scene, keeping undo history, selection state and the
    /// image cache consistent. Use this instead of assigning `document.scene`
    /// directly when loading a document from an external source.
    pub fn replace_scene(&mut self, scene: crate::core::scene::Scene, app_state: AppState) {
        self.push_checkpoint();
        self.commit_text_edit();
        self.document.scene = scene;
        self.document.selected.clear();
        self.document.zoom = app_state.zoom.value;
        self.document.scroll = Point::new(app_state.scroll_x, app_state.scroll_y);
        self.document.theme = app_state.theme;
        self.document.files = app_state.files;
        self.image_sources.clear();
        self.sync_image_sources();
        self.document.refresh_bindings();

        self.drawing = false;
        self.drag_start = None;
        self.last_point = None;
        self.current_points.clear();
        self.panning = false;
        self.pan_start = None;
        self.pan_scroll = None;
        self.drag_mode = None;
        self.resize_origin = None;
        self.marquee_start = None;
        self.marquee_current = None;
        self.lasso_path.clear();
        self.lasso_base_selection.clear();
        self.lasso_keep_selection = false;
        self.laser_points.clear();
        self.laser_active = false;
        self.pending_bucket_fill = None;
        self.pending_points.clear();
        self.erasing = false;
        self.active_locked_id = None;
        self.context_menu = None;
        self.toast = None;
    }

    /// Serializes the current scene into the `.excalidraw` JSON envelope.
    pub fn scene_json(&self) -> Result<String, String> {
        crate::export::scene_to_json(&self.document.scene, &self.app_state())
            .map_err(|err| err.to_string())
    }

    /// Loads a scene from a `.excalidraw` JSON envelope.
    pub fn load_scene_json(&mut self, json: &str) -> Result<(), String> {
        let (scene, app_state) =
            crate::export::json_to_scene(json).map_err(|err| err.to_string())?;
        self.replace_scene(scene, app_state);
        Ok(())
    }

    pub fn add_demo_scene(&mut self) {
        use crate::core::FillStyle;

        let blue = ElementOptions {
            stroke_color: "#1971c2".to_string(),
            ..Default::default()
        };
        self.document
            .scene
            .add(new_rectangle(80.0, 80.0, 200.0, 120.0, &blue));

        let red = ElementOptions {
            stroke_color: "#e03131".to_string(),
            ..Default::default()
        };
        self.document
            .scene
            .add(new_ellipse(360.0, 80.0, 140.0, 140.0, &red));

        let green = ElementOptions {
            stroke_color: "#2f9e44".to_string(),
            fill_style: FillStyle::Hachure,
            background_color: "#b2f2bb".to_string(),
            ..Default::default()
        };
        self.document
            .scene
            .add(new_diamond(560.0, 100.0, 120.0, 120.0, &green));

        let solid = ElementOptions {
            stroke_color: "#6741d9".to_string(),
            fill_style: FillStyle::Solid,
            background_color: "#d0bfff".to_string(),
            ..Default::default()
        };
        let mut rounded = new_rectangle(80.0, 460.0, 220.0, 100.0, &solid);
        if let crate::core::element::Element::Rectangle { base } = &mut rounded {
            base.roundness = Some(crate::core::types::Roundness {
                kind: crate::core::types::RoundnessType::ProportionalRadius,
                value: Some(0.25),
            });
        }
        self.document.scene.add(rounded);

        let (note, mut note_text) =
            new_sticky_note_with_text(80.0, 620.0, 160.0, 140.0, &ElementOptions::default());
        if let crate::core::element::Element::Text(t) = &mut note_text {
            t.text = "Note".to_string();
        }
        self.document.scene.add(note);
        self.document.scene.add(note_text);
        self.document.refresh_bindings();

        let rect_id = self.document.scene.elements[0].id().to_string();
        let opts = ElementOptions {
            stroke_color: "#e8590c".to_string(),
            ..Default::default()
        };
        let arrow = new_arrow(
            vec![Point::new(280.0, 140.0), Point::new(360.0, 150.0)],
            None,
            Some(Arrowhead::Arrow),
            &opts,
        );
        let arrow_id = arrow.id().to_string();
        self.document.scene.add(arrow);
        self.document.select(&arrow_id);
        let from = Point::new(360.0, 150.0);
        self.document
            .attach_linear_binding(&arrow_id, true, Some(&rect_id), from);
        self.document.clear_selection();

        self.document.scene.add(new_text(
            80.0,
            380.0,
            "Excalidraw",
            36.0,
            &ElementOptions::default(),
        ));
    }

    fn screen_point(&self, x: f32, y: f32) -> Point {
        screen_to_world(
            Point::new(x as f64, y as f64),
            self.document.zoom,
            self.document.scroll,
        )
    }

    fn commit_text_edit(&mut self) {
        if let Some(id) = self.editing_text.take() {
            self.document.clear_selection();

            let selected = match self.document.scene.get(&id) {
                Some(crate::core::element::Element::Text(t)) => {
                    t.container_id.clone().unwrap_or(id)
                }
                _ => id,
            };
            self.document.select(&selected);
        }
        self.text_edit_started = false;
        self.replace_on_type = false;
    }

    fn begin_text_edit(&mut self, id: &str, is_new: bool) {
        self.editing_text = Some(id.to_string());
        self.text_edit_started = true;
        self.replace_on_type = is_new;
        self.cursor_world = None;
    }

    pub(crate) fn push_checkpoint(&mut self) {
        self.history.push(&self.document.scene);
    }

    pub(crate) fn act<E, F>(cx: &Context<Self>, f: F) -> Handler<E>
    where
        E: ?Sized + 'static,
        F: Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    {
        Box::new(cx.listener(move |this, event, window, cx| {
            f(this, event, window, cx);
            cx.notify();
        }))
    }

    /// Records the span the opacity row's pointer maps onto, refreshed every
    /// frame by the row's `on_prepaint`.
    pub(crate) fn set_opacity_track_bounds(&mut self, bounds: gpui_kit::Bounds<Pixels>) {
        self.opacity_track_bounds = Some(bounds);
    }

    /// The 0..100 value a pointer position stands for on the opacity row's
    /// track, or `None` while the track has not been painted yet.
    fn opacity_at(&self, position: gpui_kit::Point<Pixels>) -> Option<f64> {
        let bounds = self.opacity_track_bounds?;
        let width = bounds.size.width;
        if width <= px(0.0) {
            return None;
        }
        let progress = ((position.x - bounds.left()) / width).clamp(0.0, 1.0);
        // Upstream's `Range` steps in tens.
        Some(((progress * 10.0).round() * 10.0) as f64)
    }

    /// First press of an opacity drag: the whole drag collapses into one undo
    /// step, so the checkpoint is taken here and only if the value moves.
    pub(crate) fn start_opacity_drag(&mut self, position: gpui_kit::Point<Pixels>) {
        let Some(value) = self.opacity_at(position) else {
            return;
        };
        let current = if self.bucket_style_active() {
            self.document.style.opacity
        } else {
            self.document
                .selected_elements()
                .first()
                .map(|element| element.base().opacity)
                .unwrap_or(100.0)
        };
        if (value - current).abs() < f64::EPSILON {
            return;
        }
        self.push_checkpoint();
        self.apply_opacity(value);
    }

    /// Subsequent moves of an opacity drag, applied without further checkpoints.
    pub(crate) fn drag_opacity(&mut self, position: gpui_kit::Point<Pixels>) {
        if let Some(value) = self.opacity_at(position) {
            self.apply_opacity(value);
        }
    }

    /// With the bucket armed and nothing selected the style rows edit the
    /// fill's shared settings instead of an element.
    fn bucket_style_active(&self) -> bool {
        self.document.selected.is_empty() && self.document.tool == Tool::BucketFill
    }

    fn apply_opacity(&mut self, value: f64) {
        if self.bucket_style_active() {
            self.document.style.opacity = value;
        } else {
            self.document.set_selected_opacity(value);
        }
    }

    pub fn is_interacting(&self) -> bool {
        self.panning
            || self.drawing
            || self.erasing
            || self.drag_mode.is_some()
            || self.marquee_start.is_some()
            || !self.lasso_path.is_empty()
            || self.laser_active
            || !self.pending_points.is_empty()
    }

    /// Visible part of the laser trail as `(world point, remaining fraction)`,
    /// oldest first. Each point decays linearly over `LASER_LIFETIME`, so the
    /// trail disappears from its oldest end instead of all at once.
    pub fn laser_trail(&self) -> Vec<(Point, f64)> {
        laser_trail_at(&self.laser_points, std::time::Instant::now())
    }

    fn start_laser_fade(&mut self, cx: &mut Context<Self>) {
        if self.laser_fading {
            return;
        }
        self.laser_fading = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LASER_FRAME).await;
                let keep_going = this.update(cx, |editor, cx| {
                    editor.prune_laser_points();
                    cx.notify();
                    !editor.laser_points.is_empty()
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |editor, _| editor.laser_fading = false);
        })
        .detach();
    }

    fn prune_laser_points(&mut self) {
        let now = std::time::Instant::now();
        self.laser_points
            .retain(|sample| now.saturating_duration_since(sample.born) < LASER_LIFETIME);
    }

    pub fn drag_preview(&self) -> Option<crate::core::element::Element> {
        if !self.drawing {
            return None;
        }
        let start = self.drag_start?;
        let world = self.cursor_world?;
        let (x, y, w, h) = self.drag_box(start, world, self.cursor_mods.shift);
        let opts = self.current_style_options();
        Some(match self.document.tool {
            Tool::Rectangle => new_rectangle(x, y, w, h, &opts),
            Tool::Diamond => new_diamond(x, y, w, h, &opts),
            Tool::Ellipse => new_ellipse(x, y, w, h, &opts),
            Tool::Frame => new_frame(x, y, w, h, None, &opts),
            Tool::StickyNote => new_sticky_note_with_text(x, y, w, h, &opts).0,
            Tool::Embeddable => new_embeddable(x, y, w, h, &opts),
            _ => return None,
        })
    }

    pub fn freedraw_preview(&self) -> Option<&[Point]> {
        if self.drawing && self.document.tool == Tool::FreeDraw && self.current_points.len() >= 2 {
            Some(&self.current_points)
        } else {
            None
        }
    }

    fn current_style_options(&self) -> ElementOptions {
        ElementOptions {
            stroke_color: self.document.style.stroke_color.clone(),
            background_color: self.document.style.background_color.clone(),
            fill_style: self.document.style.fill_style,
            stroke_width: self.document.style.stroke_width,
            stroke_style: self.document.style.stroke_style,
            roughness: self.document.style.roughness,
            opacity: self.document.style.opacity,
        }
    }

    /// Background color used by the bucket tool. A transparent background would
    /// paint nothing, so it falls back to the green swatch upstream uses.
    pub fn bucket_fill_color(&self) -> String {
        let color = self.document.style.background_color.clone();
        if crate::core::color::is_transparent(&color) {
            "#b2f2bb".to_string()
        } else {
            color
        }
    }

    fn commit_bucket_fill(&mut self, cx: &mut Context<Self>) {
        let Some(point) = self.pending_bucket_fill.take() else {
            return;
        };
        let background = self.bucket_fill_color();
        let elements: Vec<Element> = self.document.scene.elements.clone();

        if let Some(index) = crate::render::bucket_fill::find_fill_at(&elements, point) {
            self.restyle_bucket_fill(index, &background);
            cx.notify();
            return;
        }

        match crate::render::bucket_fill::compute_bucket_fill(point, &elements) {
            Ok(region) => {
                self.push_checkpoint();
                let options = ElementOptions {
                    stroke_color: "transparent".to_string(),
                    background_color: background,
                    fill_style: self.document.style.fill_style,
                    stroke_width: 1.0,
                    stroke_style: StrokeStyle::Solid,
                    roughness: 0.0,
                    opacity: self.document.style.opacity,
                };
                let mut fill = new_line(region.points, &options);
                if let Element::Line(line) = &mut fill {
                    line.polygon = true;
                }
                // Paint belongs just above the shape it fills: keeping it below
                // later elements (text, arrows) mirrors upstream's insertion.
                let anchor = region
                    .owner_id
                    .as_deref()
                    .and_then(|id| {
                        self.document
                            .scene
                            .elements
                            .iter()
                            .position(|e| e.id() == id)
                    })
                    .or_else(|| {
                        region
                            .boundary_ids
                            .iter()
                            .filter_map(|id| {
                                self.document
                                    .scene
                                    .elements
                                    .iter()
                                    .position(|e| e.id() == id)
                            })
                            .max()
                    });
                let index = anchor
                    .map(|index| index + 1)
                    .unwrap_or(self.document.scene.elements.len())
                    .min(self.document.scene.elements.len());
                self.document.scene.elements.insert(index, fill);
            }
            Err(BucketFillError::NoOwner) => {}
            Err(BucketFillError::TooComplex) => {
                let message = self.i18n.t("bucketfill.tooComplex");
                self.show_toast(message, false, 3000, cx);
            }
            Err(
                BucketFillError::OpenRegion
                | BucketFillError::TooSmall
                | BucketFillError::InvalidPolygon,
            ) => {
                let message = self.i18n.t("bucketfill.noRegion");
                self.show_toast(message, false, 3000, cx);
            }
        }
        cx.notify();
    }

    /// Re-applies the current bucket settings to an existing fill instead of
    /// stacking a duplicate polygon on top of it.
    fn restyle_bucket_fill(&mut self, index: usize, background: &str) {
        let opacity = self.document.style.opacity;
        let fill_style = self.document.style.fill_style;
        let changed = self
            .document
            .scene
            .elements
            .get(index)
            .filter(|element| crate::render::bucket_fill::is_fill_compatible(element))
            .map(|element| {
                let base = element.base();
                base.background_color != background
                    || base.fill_style != fill_style
                    || base.opacity != opacity
            })
            .unwrap_or(false);
        if !changed {
            return;
        }
        self.push_checkpoint();
        if let Some(element) = self.document.scene.elements.get_mut(index) {
            let base = element.base_mut();
            base.background_color = background.to_string();
            base.fill_style = fill_style;
            base.opacity = opacity;
        }
    }

    fn refresh_text_bounds(&mut self, id: &str) {
        let container_bound = matches!(
            self.document.scene.get(id),
            Some(crate::core::element::Element::Text(t)) if t.container_id.is_some()
        );
        if container_bound {
            self.document.refresh_bindings();
            return;
        }
        if let Some(crate::core::element::Element::Text(t)) = self.document.scene.get_mut(id) {
            let lines: Vec<&str> = t.text.split('\n').collect();
            let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
            t.base.width = (longest * t.font_size * 0.6).max(t.font_size * 0.6);
            t.base.height = (lines.len() as f64).max(1.0) * t.font_size * t.line_height;
        }
    }

    fn cancel_pending(&mut self) {
        self.pending_points.clear();
        self.cursor_world = None;
        self.context_menu = None;
    }

    fn element_angle(&self) -> f64 {
        let ids: Vec<String> = self.document.selected.iter().cloned().collect();
        for id in ids {
            if let Some(e) = self.document.scene.get(&id) {
                return e.base().angle;
            }
        }
        0.0
    }

    fn hit_endpoint(&self, world: Point) -> Option<(String, usize)> {
        let (id, points) = self.document.selected_linear_points()?;
        let threshold = 8.0 / self.document.zoom;
        for (i, p) in points.iter().enumerate() {
            if world.distance(*p) <= threshold {
                return Some((id.clone(), i));
            }
        }
        None
    }

    fn hit_handle(&self, world: Point) -> Option<ResizeHandle> {
        let bounds = selection_bounds(&self.document.scene, &self.document.selected)?;
        let threshold = 8.0 / self.document.zoom;
        let sx = bounds.min_x;
        let sy = bounds.min_y;
        let ex = bounds.max_x;
        let ey = bounds.max_y;
        let cx = (sx + ex) / 2.0;
        let cy = (sy + ey) / 2.0;

        let rotate_origin = Point::new(cx, sy - 24.0 / self.document.zoom);
        if world.distance(rotate_origin) <= threshold {
            return Some(ResizeHandle::Rotate);
        }

        let handles = [
            (ResizeHandle::Nw, Point::new(sx, sy)),
            (ResizeHandle::N, Point::new(cx, sy)),
            (ResizeHandle::Ne, Point::new(ex, sy)),
            (ResizeHandle::E, Point::new(ex, cy)),
            (ResizeHandle::Se, Point::new(ex, ey)),
            (ResizeHandle::S, Point::new(cx, ey)),
            (ResizeHandle::Sw, Point::new(sx, ey)),
            (ResizeHandle::W, Point::new(sx, cy)),
        ];
        for (handle, p) in handles {
            if world.distance(p) <= threshold {
                return Some(handle);
            }
        }
        None
    }

    fn duplicate_in_place(&mut self) {
        let ids: Vec<String> = self.document.selected.iter().cloned().collect();
        let mut new_ids = Vec::new();
        for id in ids {
            let src = self.document.scene.get(&id).cloned();
            if let Some(mut copy) = src {
                let nid = new_id();
                copy.base_mut().id = nid.clone();
                self.document.scene.add(copy);
                new_ids.push(nid);
            }
        }
        if !new_ids.is_empty() {
            self.document.selected.clear();
            self.document.selected.extend(new_ids);
        }
    }

    fn finish_pending_line(&mut self) {
        if self.pending_points.len() < 2 {
            self.pending_points.clear();
            self.cursor_world = None;
            return;
        }
        let pts = self.pending_points.clone();
        let opts = self.current_style_options();
        let is_line = self.document.tool == Tool::Line;
        self.push_checkpoint();
        let el = if is_line {
            new_line(pts.clone(), &opts)
        } else {
            new_arrow(pts.clone(), None, Some(Arrowhead::Arrow), &opts)
        };
        let id = el.id().to_string();
        self.document.scene.add(el);
        self.document.select(&id);

        let n = pts.len();
        let exclude = vec![id.clone()];

        if self.document.arrow_binding {
            let th = crate::core::binding::BINDING_THRESHOLD / self.document.zoom;
            let start_target =
                crate::core::binding::find_bindable(&self.document.scene, pts[0], th, &exclude);
            let end_target =
                crate::core::binding::find_bindable(&self.document.scene, pts[n - 1], th, &exclude);
            if let Some(t) = start_target {
                self.document
                    .attach_linear_binding(&id, true, Some(&t), pts[1]);
            }
            if let Some(t) = end_target {
                self.document
                    .attach_linear_binding(&id, false, Some(&t), pts[n - 2]);
            }
        }
        self.pending_points.clear();
        self.cursor_world = None;
    }

    pub fn on_canvas_mouse_down(&mut self, x: f32, y: f32, mods: Modifiers, click_count: usize) {
        let world = self.screen_point(x, y);
        self.context_menu = None;
        self.show_menu = false;
        self.show_prefs = false;
        self.show_lang = false;
        self.show_more_tools = false;

        self.cursor_hint = None;

        if let Some(active) = self.active_locked_id.clone() {
            let hit_the_target = self
                .document
                .elements_at_including_locked(world, 5.0 / self.document.zoom)
                .iter()
                .any(|element| locked_subject_id(element) == active);
            if !hit_the_target {
                self.active_locked_id = None;
            }
        }

        if self.space_pan {
            self.panning = true;
            self.pan_start = Some(Point::new(x as f64, y as f64));
            self.pan_scroll = Some(self.document.scroll);
            return;
        }

        if self.document.view_mode {
            self.panning = true;
            self.pan_start = Some(Point::new(x as f64, y as f64));
            self.pan_scroll = Some(self.document.scroll);
            return;
        }

        match self.document.tool {
            Tool::Hand => {
                self.panning = true;
                self.pan_start = Some(Point::new(x as f64, y as f64));
                self.pan_scroll = Some(self.document.scroll);
            }
            Tool::Selection => {
                if let Some((id, index)) = self.hit_endpoint(world) {
                    self.push_checkpoint();
                    self.drag_mode = Some(DragMode::Endpoint { index });
                    self.drag_target = Some(id);
                    return;
                }
                if let Some(handle) = self.hit_handle(world) {
                    self.push_checkpoint();
                    self.drag_mode = Some(match handle {
                        ResizeHandle::Rotate => {
                            let bounds =
                                selection_bounds(&self.document.scene, &self.document.selected)
                                    .unwrap_or(Bounds::new(0.0, 0.0, 0.0, 0.0));
                            let center = bounds.center();
                            self.rotate_start_angle = self.element_angle();
                            DragMode::Rotate {
                                center,
                                last: world,
                            }
                        }
                        h => {
                            let bounds =
                                selection_bounds(&self.document.scene, &self.document.selected)
                                    .unwrap_or(Bounds::new(0.0, 0.0, 0.0, 0.0));
                            self.resize_origin = Some(bounds);
                            DragMode::Resize {
                                handle: h,
                                pivot: bounds.center(),
                            }
                        }
                    });
                    return;
                }

                if let Some(id) = self.document.element_at(world, 5.0 / self.document.zoom) {
                    let already = self.document.selected.contains(&id);
                    if mods.shift && already {
                        self.document.selected.remove(&id);
                        return;
                    }
                    if mods.shift {
                        self.document.selected.insert(id.clone());
                    } else if !already {
                        self.document.select(&id);
                    }
                    self.sync_link_popup(&id);
                    if let Some(e) = self.document.scene.get(&id)
                        && e.is_text()
                        && already
                    {
                        self.begin_text_edit(&id, false);
                        return;
                    }

                    if click_count >= 2
                        && let Some(text_id) = self.document.container_text_id(&id)
                    {
                        self.begin_text_edit(&text_id, false);
                        return;
                    }
                    self.begin_drag(world, mods.alt);
                } else {
                    if !mods.shift {
                        self.document.clear_selection();
                        self.show_link_dialog = false;
                    }
                    self.marquee_start = Some(world);
                    self.marquee_current = Some(world);
                }
            }
            Tool::FreeDraw => {
                self.drawing = true;
                self.current_points = vec![world];
                self.last_point = Some(world);
                self.cursor_world = Some(world);
            }
            Tool::Text => {
                if let Some(id) = self
                    .document
                    .text_target_at(world, 5.0 / self.document.zoom)
                    && let Some(e) = self.document.scene.get(&id)
                    && e.is_text()
                {
                    self.document.select(&id);
                    self.begin_text_edit(&id, false);
                    return;
                }
                let opts = self.current_style_options();
                self.push_checkpoint();
                let el = new_text(
                    world.x,
                    world.y,
                    "Text",
                    self.document.style.font_size,
                    &opts,
                );
                let id = el.id().to_string();
                self.document.scene.add(el);
                self.document.select(&id);
                self.begin_text_edit(&id, true);
            }
            Tool::Image => {}
            Tool::Eraser => {
                self.push_checkpoint();
                self.erasing = true;
                self.erase_at(world);
            }
            Tool::Laser => {
                self.laser_points = vec![LaserPoint {
                    point: world,
                    born: std::time::Instant::now(),
                }];
                self.laser_active = true;
            }
            Tool::BucketFill => {
                self.pending_bucket_fill = Some(world);
            }
            Tool::Lasso => {
                let hit_selected = self
                    .document
                    .element_at(world, 5.0 / self.document.zoom)
                    .map(|id| self.document.selected.contains(&id))
                    .unwrap_or(false);
                let inside_selection = !self.document.selected.is_empty()
                    && selection_bounds(&self.document.scene, &self.document.selected)
                        .map(|b| b.contains(world))
                        .unwrap_or(false);
                // A press on the current selection keeps the regular drag
                // behaviour; anything else starts a new lasso path.
                if hit_selected || inside_selection {
                    self.begin_drag(world, mods.alt);
                    return;
                }
                let keep_previous = mods.shift;
                if !keep_previous {
                    self.document.clear_selection();
                    self.show_link_dialog = false;
                }
                self.lasso_keep_selection = keep_previous;
                self.lasso_base_selection = self.document.selected.iter().cloned().collect();
                self.lasso_path = vec![world];
            }
            Tool::Line | Tool::Arrow => {
                let same_as_last = self
                    .pending_points
                    .last()
                    .map(|p| p.distance(world) <= CLICK_THRESHOLD)
                    .unwrap_or(false);
                if click_count >= 2 || (same_as_last && self.pending_points.len() >= 2) {
                    self.finish_pending_line();
                    return;
                }
                if self.pending_points.is_empty() {
                    self.push_checkpoint();
                }
                self.pending_points.push(world);
                self.cursor_world = Some(world);
            }
            _ => {
                self.drawing = true;
                self.drag_start = Some(world);
                self.last_point = Some(world);
                self.current_points = vec![world];
                self.cursor_world = Some(world);
            }
        }
    }

    fn settle_active_locked(&mut self, world: Point) {
        let hits = self
            .document
            .elements_at_including_locked(world, 5.0 / self.document.zoom);
        let already_selected = hits
            .iter()
            .any(|element| self.document.selected.contains(element.id()));
        self.active_locked_id = if already_selected {
            None
        } else {
            hits.last()
                .filter(|element| element.base().locked)
                .map(|element| locked_subject_id(element))
        };
    }

    fn on_unlock_locked_target(&mut self) {
        let Some(id) = self.active_locked_id.clone() else {
            return;
        };
        let ids: Vec<String> = self
            .document
            .locked_target(&id)
            .iter()
            .map(|element| element.id().to_string())
            .collect();
        if ids.is_empty() {
            self.active_locked_id = None;
            return;
        }
        self.push_checkpoint();
        self.document.selected = ids.into_iter().collect();
        self.document.set_selected_locked(false);
        self.active_locked_id = None;
    }

    fn begin_drag(&mut self, world: Point, alt: bool) {
        self.push_checkpoint();
        if alt {
            self.duplicate_in_place();
        }
        self.drag_mode = Some(DragMode::Move { last: world });
        self.drag_start_world = Some(world);
        self.drag_anchor = selection_bounds(&self.document.scene, &self.document.selected)
            .map(|b| Point::new(b.min_x, b.min_y));
        self.drag_accum = Point::zero();
    }

    fn update_lasso_selection(&mut self) {
        let mut ids = crate::core::lasso::elements_in_lasso(&self.document.scene, &self.lasso_path);
        if self.lasso_keep_selection {
            ids.extend(self.lasso_base_selection.iter().cloned());
        }
        self.document.select_many(ids);
    }

    fn erase_at(&mut self, world: Point) {
        let threshold = 8.0 / self.document.zoom;
        if let Some(id) = self.document.element_at(world, threshold) {
            self.document.scene.remove(&id);
            self.document.selected.remove(&id);

            if let Some(text_id) = self.document.container_text_id(&id) {
                self.document.scene.remove(&text_id);
                self.document.selected.remove(&text_id);
            }
        }
    }

    pub fn on_canvas_mouse_move(&mut self, x: f32, y: f32, mods: Modifiers) {
        self.cursor_mods = mods;
        self.cursor_screen = (x as f64, y as f64);
        if self.panning {
            if let (Some(start), Some(scroll)) = (self.pan_start, self.pan_scroll) {
                self.document.scroll = Point::new(
                    scroll.x + (x as f64 - start.x),
                    scroll.y + (y as f64 - start.y),
                );
            }
            return;
        }
        let world = self.screen_point(x, y);
        self.cursor_world = Some(world);

        if self.erasing {
            self.erase_at(world);
            return;
        }

        if !self.lasso_path.is_empty() {
            let min_step = 2.0 / self.document.zoom;
            if self
                .lasso_path
                .last()
                .map(|last| last.distance(world) >= min_step)
                .unwrap_or(true)
            {
                self.lasso_path.push(world);
            }
            self.update_lasso_selection();
            return;
        }

        if let Some(mode) = self.drag_mode {
            match mode {
                DragMode::Move { .. } => self.apply_move(world, mods),
                DragMode::Resize { handle, pivot } => {
                    self.apply_resize(handle, pivot, world, mods.shift)
                }
                DragMode::Rotate { center, last } => {
                    let prev = (last.y - center.y).atan2(last.x - center.x);
                    let now = (world.y - center.y).atan2(world.x - center.x);
                    let mut delta = now - prev;
                    if mods.shift {
                        let step = 15.0_f64.to_radians();
                        delta = (delta / step).round() * step;
                    }
                    self.document.rotate_selected(delta, center);
                    self.drag_mode = Some(DragMode::Rotate {
                        center,
                        last: world,
                    });
                }
                DragMode::Endpoint { index } => {
                    if let Some(id) = self.drag_target.clone() {
                        self.document
                            .move_linear_endpoint(&id, index, self.document.snap(world));
                    }
                }
            }
            return;
        }

        if let Some(start) = self.marquee_start {
            self.marquee_current = Some(world);
            let b = Bounds::new(
                start.x.min(world.x),
                start.y.min(world.y),
                start.x.max(world.x),
                start.y.max(world.y),
            );
            let ids: Vec<String> = self
                .document
                .scene
                .non_deleted()
                .filter(|e| !e.base().locked)
                .filter(|e| !crate::state::is_bound_text(e))
                .filter(|e| {
                    let eb = crate::core::bounds::element_bounds(e);

                    if self.document.select_wrap {
                        b.min_x <= eb.min_x
                            && b.min_y <= eb.min_y
                            && b.max_x >= eb.max_x
                            && b.max_y >= eb.max_y
                    } else {
                        b.intersects(&eb)
                    }
                })
                .map(|e| e.id().to_string())
                .collect();
            self.document.select_many(ids);
            return;
        }

        if self.document.tool == Tool::Laser {
            if self.laser_active {
                self.laser_points.push(LaserPoint {
                    point: world,
                    born: std::time::Instant::now(),
                });
            }
            return;
        }

        if self.drawing {
            self.current_points.push(world);
            self.last_point = Some(world);
        }
    }

    fn apply_move(&mut self, world: Point, mods: Modifiers) {
        let (Some(start), Some(anchor)) = (self.drag_start_world, self.drag_anchor) else {
            return;
        };
        let mut raw = Point::new(world.x - start.x, world.y - start.y);
        if mods.shift {
            if raw.x.abs() >= raw.y.abs() {
                raw.y = 0.0;
            } else {
                raw.x = 0.0;
            }
        }
        let target = Point::new(anchor.x + raw.x, anchor.y + raw.y);
        let snapped = self.document.snap(target);
        let delta = Point::new(snapped.x - anchor.x, snapped.y - anchor.y);
        let inc = Point::new(delta.x - self.drag_accum.x, delta.y - self.drag_accum.y);
        if inc.x != 0.0 || inc.y != 0.0 {
            self.document.move_selected(inc.x, inc.y);
        }
        self.drag_accum = delta;
    }

    fn apply_resize(&mut self, handle: ResizeHandle, _pivot: Point, world: Point, shift: bool) {
        let Some(origin) = self.resize_origin else {
            return;
        };
        let (sx, sy) = (origin.min_x, origin.min_y);
        let (ex, ey) = (origin.max_x, origin.max_y);
        let w = origin.width();
        let h = origin.height();

        let (anchor_x, anchor_y) = match handle {
            ResizeHandle::Nw | ResizeHandle::N => (ex, ey),
            ResizeHandle::Ne | ResizeHandle::E => (sx, ey),
            ResizeHandle::Se | ResizeHandle::S => (sx, sy),
            ResizeHandle::Sw | ResizeHandle::W => (ex, sy),
            ResizeHandle::Rotate => return,
        };

        let mut new_w = match handle {
            ResizeHandle::Nw | ResizeHandle::Sw | ResizeHandle::W => (anchor_x - world.x).abs(),
            ResizeHandle::Ne | ResizeHandle::Se | ResizeHandle::E => (world.x - anchor_x).abs(),
            _ => w,
        };
        let mut new_h = match handle {
            ResizeHandle::Nw | ResizeHandle::Ne | ResizeHandle::N => (anchor_y - world.y).abs(),
            ResizeHandle::Sw | ResizeHandle::Se | ResizeHandle::S => (world.y - anchor_y).abs(),
            _ => h,
        };

        if shift && w > 0.0 && h > 0.0 {
            let ratio = w / h;
            if new_w / w >= new_h / h {
                new_h = new_w / ratio;
            } else {
                new_w = new_h * ratio;
            }
        }

        let scale_x = if w > 0.0 { new_w / w } else { 1.0 };
        let scale_y = if h > 0.0 { new_h / h } else { 1.0 };

        if scale_x > 0.0 && scale_y > 0.0 && (scale_x != 1.0 || scale_y != 1.0) {
            let anchor = Point::new(anchor_x, anchor_y);
            self.document.resize_selected(scale_x, scale_y, anchor);

            self.resize_origin = selection_bounds(&self.document.scene, &self.document.selected);
        }
    }

    pub fn on_canvas_mouse_up(&mut self, x: f32, y: f32, mods: Modifiers) {
        let world = self.screen_point(x, y);

        if !self.panning && !self.erasing && self.document.tool == Tool::Selection {
            let marquee_dragged = self
                .marquee_start
                .map(|start| start.distance(world) > CLICK_THRESHOLD)
                .unwrap_or(false);
            if !marquee_dragged {
                self.settle_active_locked(world);
            }
        }
        if self.panning {
            self.panning = false;
            self.pan_start = None;
            self.pan_scroll = None;
            return;
        }

        if self.erasing {
            self.erasing = false;
            return;
        }

        if !self.lasso_path.is_empty() {
            // The selection made along the path is kept; only the trail goes.
            self.lasso_path.clear();
            self.lasso_base_selection.clear();
            self.lasso_keep_selection = false;
            return;
        }

        if self.drag_mode.is_some() {
            self.drag_mode = None;
            self.resize_origin = None;
            self.drag_start_world = None;
            self.drag_anchor = None;
            self.drag_accum = Point::zero();
            self.drag_target = None;
            return;
        }

        if self.marquee_start.is_some() {
            self.marquee_start = None;
            self.marquee_current = None;
            return;
        }

        if self.document.tool == Tool::Laser {
            // The trail keeps its points on release; the fade ticker retires
            // them one by one from the oldest end.
            self.laser_active = false;
            return;
        }

        if matches!(self.document.tool, Tool::Line | Tool::Arrow) {
            if let Some(start) = self.pending_points.first().copied()
                && self.pending_points.len() == 1
                && start.distance(world) > CLICK_THRESHOLD
            {
                let end = if mods.shift {
                    crate::core::binding::constrain_angle(start, world, 15.0)
                } else {
                    self.document.snap(world)
                };
                self.pending_points.push(end);
                self.finish_pending_line();
            }
            return;
        }

        if !self.drawing {
            return;
        }
        self.drawing = false;

        let opts = self.current_style_options();
        self.push_checkpoint();

        let start = self.drag_start;
        match self.document.tool {
            Tool::FreeDraw => {
                let pts = streamline_points(&self.current_points, 0.35);
                self.document.scene.add(new_freedraw(pts, &opts));
            }
            Tool::Rectangle | Tool::Diamond | Tool::Ellipse | Tool::Frame => {
                if let Some(start) = start {
                    let (x, y, w, h) = self.drag_box(start, world, mods.shift);
                    let el = match self.document.tool {
                        Tool::Rectangle => new_rectangle(x, y, w, h, &opts),
                        Tool::Diamond => new_diamond(x, y, w, h, &opts),
                        Tool::Ellipse => new_ellipse(x, y, w, h, &opts),
                        _ => new_frame(x, y, w, h, None, &opts),
                    };
                    self.document.scene.add(el);
                }
            }
            Tool::StickyNote => {
                if let Some(start) = start {
                    let (x, y, w, h) = self.drag_box(start, world, mods.shift);
                    let (note, text) = new_sticky_note_with_text(x, y, w, h, &opts);
                    self.document.scene.add(note);
                    self.document.scene.add(text);
                    self.document.refresh_bindings();
                }
            }
            Tool::Embeddable => {
                if let Some(start) = start {
                    let (x, y, w, h) = self.drag_box(start, world, mods.shift);
                    let element = new_embeddable(x, y, w, h, &opts);
                    let id = element.id().to_string();
                    self.document.scene.add(element);
                    // Upstream opens the link editor right after the element
                    // is drawn, since an embeddable is meaningless without it.
                    self.document.select(&id);
                    self.open_link_dialog();
                }
            }
            _ => {}
        }

        self.drag_start = None;
        self.current_points.clear();
        self.last_point = None;

        if !self.document.tool_locked
            && matches!(
                self.document.tool,
                Tool::Rectangle
                    | Tool::Diamond
                    | Tool::Ellipse
                    | Tool::FreeDraw
                    | Tool::Frame
                    | Tool::StickyNote
                    | Tool::Embeddable
            )
        {
            self.set_tool(Tool::Selection);
        }
    }

    fn drag_box(&self, start: Point, world: Point, shift: bool) -> (f64, f64, f64, f64) {
        let mut end = self.document.snap(world);
        if shift {
            end = crate::core::binding::constrain_square(start, end);
        }
        let mut w = (end.x - start.x).abs();
        let mut h = (end.y - start.y).abs();
        if w < CLICK_THRESHOLD && h < CLICK_THRESHOLD {
            w = CLICK_DEFAULT_SIZE;
            h = CLICK_DEFAULT_SIZE;
        }
        let x = start.x.min(end.x);
        let y = start.y.min(end.y);
        (x, y, w.max(1.0), h.max(1.0))
    }

    pub(crate) fn delete_selected(&mut self) {
        self.on_delete();
    }

    pub(crate) fn duplicate_selected(&mut self) {
        self.on_duplicate();
    }

    pub(crate) fn group_selected(&mut self) {
        self.on_group();
    }

    pub(crate) fn align_selected(&mut self, align: crate::core::operations::Align) {
        self.on_align(align);
    }

    pub(crate) fn distribute_selected(&mut self, dir: crate::core::operations::Distribute) {
        self.on_distribute(dir);
    }

    pub(crate) fn lock_selected(&mut self, locked: bool) {
        if locked {
            self.on_lock();
        } else {
            self.on_unlock();
        }
    }

    pub fn on_undo(&mut self) {
        if self.history.undo(&mut self.document.scene) {
            self.document.clear_selection();
            self.document.refresh_bindings();
        }
    }

    fn on_redo(&mut self) {
        if self.history.redo(&mut self.document.scene) {
            self.document.clear_selection();
            self.document.refresh_bindings();
        }
    }

    fn on_delete(&mut self) {
        if self.document.selected.is_empty() {
            return;
        }
        self.push_checkpoint();
        self.document.delete_selected();
        self.active_locked_id = None;
    }

    fn on_duplicate(&mut self) {
        if self.document.selected.is_empty() {
            return;
        }
        self.push_checkpoint();
        self.document.duplicate_selected();
    }

    fn on_copy(&mut self) {
        self.document.copy_selected();
        self.status = Some(format!(
            "{} {}",
            self.i18n.t("ctx.copy"),
            self.document.clipboard.len()
        ));
    }

    fn on_cut(&mut self) {
        if self.document.selected.is_empty() {
            return;
        }
        self.push_checkpoint();
        self.document.cut_selected();
    }

    fn on_paste(&mut self) {
        if !self.document.has_clipboard() {
            return;
        }
        self.push_checkpoint();
        self.document.paste(20.0, 20.0);
    }

    pub fn sync_image_sources(&mut self) {
        let mut missing: Vec<(String, String, String)> = Vec::new();
        for (id, file) in &self.document.files {
            if !self.image_sources.contains_key(id) {
                missing.push((id.clone(), file.mime_type.clone(), file.data_url.clone()));
            }
        }
        for (id, mime, url) in missing {
            if let Some(bytes) = crate::bitmap::bytes_from_data_url(&url)
                && let Some(render) = crate::bitmap::decode_render_image(&bytes, &mime)
            {
                self.image_sources.insert(id, render);
            }
        }
        let known: Vec<String> = self.document.files.keys().cloned().collect();
        self.image_sources.retain(|id, _| known.contains(id));
    }

    pub fn register_image_file(&mut self, bytes: &[u8], mime: &str) -> String {
        let id = format!("file-{}", uuid::Uuid::new_v4());
        let mut file = crate::core::scene::BinaryFileData::new(
            id.clone(),
            mime,
            crate::bitmap::data_url(mime, bytes),
        );
        file.created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0);
        self.document.files.insert(id.clone(), file);
        if let Some(render) = crate::bitmap::decode_render_image(bytes, mime) {
            self.image_sources.insert(id.clone(), render);
        }
        id
    }

    pub fn image_dimensions(&self, file_id: &str) -> Option<(f64, f64)> {
        self.image_sources
            .get(file_id)
            .map(|r| crate::bitmap::render_image_size(r))
    }

    fn default_insert_point(&self, width: f64, height: f64) -> Point {
        let (vw, vh) = self.viewport;
        let center = screen_to_world(
            Point::new(vw / 2.0, vh / 2.0),
            self.document.zoom,
            self.document.scroll,
        );
        Point::new(center.x - width / 2.0, center.y - height / 2.0)
    }

    pub fn insert_image_bytes(
        &mut self,
        bytes: &[u8],
        mime: &str,
        at: Option<Point>,
    ) -> Option<String> {
        if bytes.is_empty() {
            return None;
        }
        let file_id = self.register_image_file(bytes, mime);
        let (width, height) = self
            .image_dimensions(&file_id)
            .map(|(w, h)| crate::bitmap::fit_size(w, h))
            .unwrap_or((200.0, 150.0));
        let origin = at.unwrap_or_else(|| self.default_insert_point(width, height));
        let opts = self.current_style_options();
        self.push_checkpoint();
        let mut element = new_image(origin.x, origin.y, width, height, Some(file_id), &opts);
        if let Element::Image(image) = &mut element {
            image.status = crate::core::element::ImageStatus::Saved;
        }
        let id = element.id().to_string();
        self.document.scene.add(element);
        self.document.select(&id);
        self.status = Some(format!(
            "{} {}x{}",
            self.i18n.t("toolbar.image"),
            width.round(),
            height.round()
        ));
        Some(id)
    }

    pub fn paste_image_from_clipboard(&mut self, cx: &App) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        for entry in &item.entries {
            if let ClipboardEntry::Image(image) = entry {
                let mime = crate::bitmap::detect_mime(&image.bytes, None);
                return self.insert_image_bytes(&image.bytes, &mime, None).is_some();
            }
        }
        false
    }

    pub fn paste_text_from_clipboard(&mut self, cx: &App) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }

        let text = text.replace("\r\n", "\n").replace('\r', "\n");

        if let Some(id) = self.editing_text.clone() {
            if let Some(crate::core::element::Element::Text(t)) = self.document.scene.get_mut(&id) {
                if self.replace_on_type {
                    t.text.clear();
                }
                t.text.push_str(&text);
            }
            self.replace_on_type = false;
            self.refresh_text_bounds(&id);
            return true;
        }

        self.paste_text_as_elements(&text);
        true
    }

    fn paste_text_as_elements(&mut self, text: &str) {
        let origin = self.cursor_world.unwrap_or_else(|| {
            let (vw, vh) = self.viewport;
            screen_to_world(
                Point::new(vw / 2.0, vh / 2.0),
                self.document.zoom,
                self.document.scroll,
            )
        });
        let options = self.current_style_options();
        let font_size = self.document.style.font_size;
        let mut ids = Vec::new();
        let mut y = origin.y;
        let mut has_text_above = false;
        for line in text.split('\n') {
            let line = line.trim();
            if line.is_empty() {
                if has_text_above {
                    y += font_size * 1.25 + PASTE_LINE_GAP;
                    has_text_above = false;
                }
                continue;
            }
            let element = new_text(0.0, 0.0, line, font_size, &options);
            let id = element.id().to_string();
            self.document.scene.add(element);

            self.refresh_text_bounds(&id);
            let Some(bounds) = self.document.scene.get(&id).map(|e| {
                let base = e.base();
                (base.width, base.height)
            }) else {
                continue;
            };
            let (w, h) = bounds;

            if let Some(element) = self.document.scene.get_mut(&id) {
                let base = element.base_mut();
                base.x = origin.x - w / 2.0;
                base.y = y - h / 2.0;
            }
            ids.push(id);
            y += h + PASTE_LINE_GAP;
            has_text_above = true;
        }
        if !ids.is_empty() {
            self.document.select_many(ids);
        }
    }

    pub fn insert_image_paths(&mut self, paths: &[std::path::PathBuf]) -> usize {
        let mut inserted = 0;
        for path in paths {
            if path.is_dir() {
                continue;
            }
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let mime = crate::bitmap::detect_mime(&bytes, path.to_str());
            let offset = 24.0 * inserted as f64;
            let at = {
                let (vw, vh) = self.viewport;
                let center = screen_to_world(
                    Point::new(vw / 2.0 + offset, vh / 2.0 + offset),
                    self.document.zoom,
                    self.document.scroll,
                );
                Some(center)
            };
            if self.insert_image_bytes(&bytes, &mime, at).is_some() {
                inserted += 1;
            }
        }
        inserted
    }

    fn open_image_picker(&mut self, cx: &mut Context<Self>) {
        let prompt = SharedString::from(self.i18n.t("toolbar.image"));
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(prompt),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                let _ = this.update(cx, |editor, _| {
                    editor.insert_image_paths(&paths);
                });
            }
            let _ = this.update(cx, |editor, _| {
                if editor.document.tool == Tool::Image {
                    editor.set_tool(Tool::Selection);
                }
            });
        })
        .detach();
    }

    fn on_zoom_in(&mut self) {
        self.document.zoom = (self.document.zoom * 1.2).min(8.0);
    }

    fn on_zoom_out(&mut self) {
        self.document.zoom = (self.document.zoom / 1.2).max(0.05);
    }

    fn on_reset_zoom(&mut self) {
        self.document.zoom = 1.0;
        self.document.scroll = Point::zero();
    }

    fn on_toggle_theme(&mut self) {
        let next = match self.document.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        self.document.theme = next;
    }

    fn on_toggle_grid(&mut self) {
        self.document.show_grid = !self.document.show_grid;
    }

    fn on_toggle_library(&mut self) {
        self.document.show_library = !self.document.show_library;
    }

    fn on_group(&mut self) {
        if self.document.selected.len() < 2 {
            return;
        }
        self.push_checkpoint();
        self.document.group_selected();
    }

    fn on_ungroup(&mut self) {
        if self.document.selected.is_empty() {
            return;
        }
        self.push_checkpoint();
        self.document.ungroup_selected();
    }

    fn on_lock(&mut self) {
        self.push_checkpoint();
        self.document.set_selected_locked(true);
        self.active_locked_id = None;
    }

    fn on_unlock(&mut self) {
        self.push_checkpoint();
        self.document.set_selected_locked(false);
        self.active_locked_id = None;
    }

    fn on_convert_target(&mut self, target: ConvertTarget) {
        let changes = self
            .document
            .lone_selected_element()
            .map(|e| {
                e.convert_target() != Some(target) && e.convertible_targets().contains(&target)
            })
            .unwrap_or(false);
        if !changes {
            return;
        }
        self.push_checkpoint();
        self.document.convert_selected_to(target);
    }

    fn on_align(&mut self, align: crate::core::operations::Align) {
        if self.document.selected.len() < 2 {
            return;
        }
        self.push_checkpoint();
        self.document.align_selected(align);
    }

    fn on_distribute(&mut self, dir: crate::core::operations::Distribute) {
        if self.document.selected.len() < 3 {
            return;
        }
        self.push_checkpoint();
        self.document.distribute_selected(dir);
    }

    pub fn selected_count(&self) -> usize {
        self.document.selected.len()
    }

    pub fn is_menu_open(&self) -> bool {
        self.show_menu
    }

    pub fn set_menu_open(&mut self, open: bool) {
        self.show_menu = open;
        if open {
            self.show_more_tools = false;
        } else {
            self.show_prefs = false;
            self.show_lang = false;
        }

        if self.color_popup == Some(ColorTarget::CanvasBackground) {
            self.close_color_popup();
        }
    }

    pub fn is_language_open(&self) -> bool {
        self.show_lang
    }

    fn menu_max_height(&self) -> f32 {
        (self.viewport.1 as f32 - size::CONTAINER_PADDING * 2.0 - size::BUTTON_LG).max(0.0)
    }

    pub fn set_more_tools_open(&mut self, open: bool) {
        self.show_more_tools = open;
        if open {
            self.show_menu = false;
        }
    }

    pub fn is_more_tools_open(&self) -> bool {
        self.show_more_tools
    }

    pub fn is_palette_open(&self) -> bool {
        self.show_palette
    }

    pub fn is_find_open(&self) -> bool {
        self.show_find
    }

    pub fn is_help_open(&self) -> bool {
        self.show_help
    }

    pub fn palette_query(&self) -> &str {
        &self.palette_query
    }

    pub fn palette_index(&self) -> usize {
        self.palette_index
    }

    pub fn set_palette_query(&mut self, query: impl Into<String>) {
        self.palette_query = query.into();
        self.palette_index = 0;
    }

    pub fn open_palette(&mut self) {
        self.show_palette = true;
        self.palette_query.clear();
        self.palette_index = 0;
        self.show_menu = false;
        self.show_more_tools = false;
    }

    pub fn toggle_palette(&mut self) {
        if self.show_palette {
            self.show_palette = false;
        } else {
            self.open_palette();
        }
    }

    /// Commands that pass both the query and upstream's availability
    /// predicate, in upstream's category order. The table is already grouped by
    /// category, so a stable sort preserves the order inside each one.
    fn palette_results(&self) -> Vec<&'static PaletteCommand> {
        let needle = self.palette_needle();
        let mut hits: Vec<&PaletteCommand> = PALETTE_COMMANDS
            .iter()
            .filter(|command| command.available(self))
            // Website-only rows (live collaboration, Excalidraw+) do nothing here.
            .filter(|command| command.action.is_some())
            .filter(|command| self.palette_matches(command, &needle))
            .collect();
        hits.sort_by_key(|command| command.category.order());
        hits
    }

    fn palette_needle(&self) -> String {
        self.palette_query.trim().to_lowercase()
    }

    fn palette_matches(&self, command: &PaletteCommand, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        self.i18n.t(command.label).to_lowercase().contains(needle)
            || command.id.contains(needle)
            || command.keywords.contains(needle)
    }

    /// The command upstream repeats in its "Recents" section: only on an empty
    /// query, and only while it is still available.
    fn palette_recent(&self) -> Option<&'static PaletteCommand> {
        if !self.palette_needle().is_empty() {
            return None;
        }
        let id = self.palette_last_used?;
        PALETTE_COMMANDS
            .iter()
            .find(|command| command.id == id && command.available(self))
    }

    /// The sections the palette paints, in order: an optional Recents section
    /// followed by the query's category groups.
    fn palette_sections(&self) -> Vec<(&'static str, Vec<&'static PaletteCommand>)> {
        let recent = self.palette_recent();
        let mut sections: Vec<(&'static str, Vec<&PaletteCommand>)> = Vec::new();
        if let Some(recent) = recent {
            sections.push(("commandPalette.recents", vec![recent]));
        }
        for command in self.palette_results() {
            if Some(command.id) == recent.map(|recent| recent.id) {
                continue;
            }
            match sections.last_mut() {
                Some((title, items)) if *title == command.category.title() => items.push(command),
                _ => sections.push((command.category.title(), vec![command])),
            }
        }
        sections
    }

    /// The rows in visual order, which is also the order the arrow keys walk.
    fn palette_display(&self) -> Vec<&'static PaletteCommand> {
        self.palette_sections()
            .into_iter()
            .flat_map(|(_, items)| items)
            .collect()
    }

    pub fn palette_labels() -> Vec<&'static str> {
        PALETTE_COMMANDS.iter().map(|c| c.label).collect()
    }

    pub fn help_labels() -> Vec<&'static str> {
        let mut keys = Vec::new();
        for &(caption, rows) in HELP_ISLANDS.iter() {
            keys.push(caption);
            for &(label, shortcuts, _) in rows {
                keys.push(label);
                for shortcut in shortcuts {
                    for part in shortcut.split('+') {
                        if part.starts_with("helpDialog.") {
                            keys.push(part);
                        }
                    }
                }
            }
        }
        keys
    }

    fn run_palette_selection(&mut self) {
        let picked = self
            .palette_display()
            .get(self.palette_index)
            .map(|command| (command.id, command.action));
        // Rows upstream ships without a local handler are listed but inert, so
        // picking or confirming one runs nothing and leaves the dialog up.
        let Some((id, Some(action))) = picked else {
            return;
        };
        self.show_palette = false;
        self.palette_last_used = Some(id);
        action.run(self);
    }

    pub fn open_find(&mut self) {
        self.show_find = true;
        self.show_menu = false;
        self.show_more_tools = false;
        self.recompute_find();
    }

    pub fn find_query(&self) -> &str {
        &self.find_query
    }

    pub fn find_match_count(&self) -> usize {
        self.find_matches.len()
    }

    pub fn find_index(&self) -> usize {
        self.find_index
    }

    pub fn set_find_query(&mut self, query: impl Into<String>) {
        self.find_query = query.into();
        self.recompute_find();
    }

    fn recompute_find(&mut self) {
        let needle = self.find_query.trim().to_lowercase();
        self.find_matches = if needle.is_empty() {
            Vec::new()
        } else {
            self.document
                .scene
                .non_deleted()
                .filter(|e| {
                    element_search_text(e).is_some_and(|s| s.to_lowercase().contains(&needle))
                })
                .map(|e| e.id().to_string())
                .collect()
        };
        self.find_index = 0;
        self.focus_find_match();
    }

    fn step_find(&mut self, forward: bool) {
        if self.find_matches.is_empty() {
            return;
        }
        let len = self.find_matches.len();
        self.find_index = if forward {
            (self.find_index + 1) % len
        } else {
            (self.find_index + len - 1) % len
        };
        self.focus_find_match();
    }

    fn focus_find_match(&mut self) {
        let Some(id) = self.find_matches.get(self.find_index).cloned() else {
            return;
        };
        self.document.select(&id);
        let Some(element) = self.document.scene.get(&id) else {
            return;
        };
        let b = crate::core::bounds::element_bounds(element);
        let (vw, vh) = self.viewport;
        self.document.scroll = Point::new(
            vw / 2.0 - (b.min_x + b.width() / 2.0) * self.document.zoom,
            vh / 2.0 - (b.min_y + b.height() / 2.0) * self.document.zoom,
        );
    }

    pub fn open_help(&mut self) {
        self.show_help = true;
        self.show_menu = false;
        self.show_more_tools = false;
    }

    #[doc(hidden)]
    pub fn debug_open_prefs(&mut self) {
        self.set_menu_open(true);
        self.show_prefs = true;
    }

    pub fn pending_point_count(&self) -> usize {
        self.pending_points.len()
    }

    pub fn scene_open_pending(&self) -> bool {
        self.pending_scene_open
    }

    pub fn save_dialog_open(&self) -> bool {
        self.show_save_dialog
    }

    pub fn scene_save_pending(&self) -> bool {
        self.pending_scene_save
    }

    pub fn save_dialog_pending(&self) -> bool {
        self.pending_save_dialog
    }

    pub fn project_name_value(&self) -> String {
        if self.project_name.is_empty() {
            self.untitled_name()
        } else {
            self.project_name.clone()
        }
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn editing_text_id(&self) -> Option<&str> {
        self.editing_text.as_deref()
    }

    pub fn has_text_selected(&self) -> bool {
        self.document
            .selected_elements()
            .iter()
            .any(|e| e.is_text())
    }

    pub fn export_background(&self) -> &str {
        if self.document.view_background_color == "#ffffff" {
            match self.document.theme {
                Theme::Light => "#ffffff",
                Theme::Dark => "#121212",
            }
        } else {
            &self.document.view_background_color
        }
    }

    pub fn export_svg_string(&self) -> String {
        crate::export::export_svg_with_files(
            &self.document.scene,
            self.export_background(),
            &self.document.files,
        )
    }

    fn app_state(&self) -> AppState {
        AppState {
            theme: self.document.theme,
            scroll_x: self.document.scroll.x,
            scroll_y: self.document.scroll.y,
            zoom: crate::core::scene::Zoom {
                value: self.document.zoom,
            },
            files: self.document.files.clone(),
            ..Default::default()
        }
    }

    pub fn save_to(&mut self, path: &str) -> Result<(), String> {
        let json = crate::export::scene_to_json(&self.document.scene, &self.app_state())
            .map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        self.status = Some(format!("saved: {path}"));
        Ok(())
    }

    pub fn load_from(&mut self, path: &str) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let (scene, app) = crate::export::json_to_scene(&text).map_err(|e| e.to_string())?;
        self.push_checkpoint();
        self.document.scene = scene;
        self.document.selected.clear();
        self.document.zoom = app.zoom.value;
        self.document.scroll = Point::new(app.scroll_x, app.scroll_y);
        self.document.theme = app.theme;
        self.document.files = app.files;
        self.image_sources.clear();
        self.sync_image_sources();
        self.document.refresh_bindings();
        self.status = Some(format!("loaded: {path}"));
        Ok(())
    }

    pub fn open_scene_picker(&mut self, cx: &mut Context<Self>) {
        let prompt = SharedString::from(self.i18n.t("menu.open"));
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.first() {
                    let path = path.to_string_lossy().to_string();
                    let _ = this.update(cx, |editor, cx| {
                        editor.load_scene_file(&path);
                        cx.notify();
                    });
                }
            }

            Ok(Err(_)) => {
                let _ = this.update(cx, |editor, cx| {
                    editor.status = Some(editor.i18n.t("status.unavailable"));
                    cx.notify();
                });
            }

            _ => {}
        })
        .detach();
    }

    pub fn load_scene_file(&mut self, path: &str) {
        if let Err(err) = self.load_from(path) {
            self.status = Some(format!("open failed: {err}"));
        }
    }

    fn untitled_name(&self) -> String {
        let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M");
        format!("{}-{stamp}", self.i18n.t("labels.untitled"))
    }

    pub fn open_save_dialog(&mut self) {
        if self.project_name.is_empty() {
            self.project_name = self.untitled_name();
        }
        self.name_caret = self.project_name.len();
        self.name_anchor = self.name_caret;
        self.show_save_dialog = true;
    }

    pub fn close_save_dialog(&mut self) {
        self.show_save_dialog = false;
    }

    pub fn save_scene_file(&mut self, path: &str) {
        match self.save_to(path) {
            Ok(()) => self.show_save_dialog = false,
            Err(err) => self.status = Some(format!("save failed: {err}")),
        }
    }

    pub fn save_scene_picker(&mut self, cx: &mut Context<Self>) {
        let name = self.project_name_value();
        let name = if name.ends_with(".excalidraw") {
            name
        } else {
            format!("{name}.excalidraw")
        };
        let directory = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(path))) => {
                let path = path.to_string_lossy().to_string();
                let _ = this.update(cx, |editor, cx| {
                    editor.save_scene_file(&path);
                    cx.notify();
                });
            }

            Ok(Err(_)) => {
                let _ = this.update(cx, |editor, cx| {
                    editor.status = Some(editor.i18n.t("status.unavailable"));
                    cx.notify();
                });
            }

            _ => {}
        })
        .detach();
    }

    pub fn image_dialog_open(&self) -> bool {
        self.show_image_dialog
    }

    pub fn image_dialog_pending(&self) -> bool {
        self.pending_image_dialog
    }

    pub fn export_scale(&self) -> f32 {
        self.export_scale
    }

    pub fn export_with_background(&self) -> bool {
        self.export_with_background
    }

    pub fn export_dark_mode(&self) -> bool {
        self.export_dark_mode
    }

    pub fn export_embed_scene(&self) -> bool {
        self.export_embed_scene
    }

    pub fn export_only_selected(&self) -> bool {
        self.export_only_selected
    }

    pub fn open_image_dialog(&mut self) {
        if self.project_name.is_empty() {
            self.project_name = self.untitled_name();
        }
        self.name_caret = self.project_name.len();
        self.name_anchor = self.name_caret;
        self.show_image_dialog = true;
        self.refresh_export_preview();
    }

    pub fn close_image_dialog(&mut self) {
        self.show_image_dialog = false;
        self.export_preview = None;
    }

    pub fn is_mermaid_dialog_open(&self) -> bool {
        self.show_mermaid_dialog
    }

    pub fn mermaid_source(&self) -> &str {
        &self.mermaid_text
    }

    pub fn mermaid_preview_elements(&self) -> &[Element] {
        &self.mermaid_preview
    }

    pub fn open_mermaid_dialog(&mut self) {
        self.mermaid_tab = MermaidTab::Mermaid;
        if self.mermaid_text.is_empty() {
            self.mermaid_text = MERMAID_EXAMPLE.to_string();
        }
        self.mermaid_caret = self.mermaid_text.len();
        self.mermaid_anchor = self.mermaid_caret;
        self.show_mermaid_dialog = true;
        self.refresh_mermaid_preview();
    }

    pub fn close_mermaid_dialog(&mut self) {
        self.show_mermaid_dialog = false;
        self.mermaid_preview.clear();
        self.mermaid_error = None;
    }

    /// Re-parses the editor buffer, mirroring upstream's live preview. A parse
    /// failure clears the preview and keeps the message for the error pane.
    fn refresh_mermaid_preview(&mut self) {
        if self.mermaid_text.trim().is_empty() {
            self.mermaid_preview.clear();
            self.mermaid_error = None;
            return;
        }
        match crate::core::mermaid::mermaid_elements(&self.mermaid_text) {
            Ok(elements) => {
                self.mermaid_preview = elements;
                self.mermaid_error = None;
            }
            Err(message) => {
                self.mermaid_preview.clear();
                self.mermaid_error = Some(message);
            }
        }
    }

    /// Drops the generated diagram into the scene, centered in the viewport and
    /// selected, then closes the dialog like upstream's `insertToEditor`.
    pub fn insert_mermaid_diagram(&mut self) -> bool {
        if self.mermaid_preview.is_empty() {
            return false;
        }
        let mut elements = self.mermaid_preview.clone();
        let refs: Vec<&Element> = elements.iter().collect();
        let Some(bounds) = crate::core::bounds::common_bounds(&refs) else {
            return false;
        };
        let width = (bounds.max_x - bounds.min_x).max(1.0);
        let height = (bounds.max_y - bounds.min_y).max(1.0);
        // Upstream inserts with `fit: "scale-down"`: it zooms the viewport out
        // (never past 100%) so the whole diagram is visible, then centres it.
        let (vw, vh) = self.viewport;
        let fit_zoom = ((vw / width).min(vh / height)).min(1.0);
        self.document.zoom = fit_zoom.clamp(0.05, 8.0);
        let origin = self.default_insert_point(width, height);
        for element in &mut elements {
            crate::core::transform::translate_element(
                element,
                origin.x - bounds.min_x,
                origin.y - bounds.min_y,
            );
        }

        self.push_checkpoint();
        let ids: Vec<String> = elements
            .iter()
            .filter(
                |element| !matches!(element, Element::Text(text) if text.container_id.is_some()),
            )
            .map(|element| element.id().to_string())
            .collect();
        for element in elements {
            self.document.scene.add(element);
        }
        self.document.refresh_bindings();
        self.document.select_many(ids);
        self.close_mermaid_dialog();
        true
    }

    /// The elements the export dialog is currently pointed at: the whole scene,
    /// or just the selection when "only selected" is on.
    fn export_scene(&self) -> Scene {
        if !self.export_only_selected {
            return self.document.scene.clone();
        }
        let mut scene = Scene::new();
        for element in self.document.scene.non_deleted() {
            if self.document.selected.contains(element.id()) {
                scene.add(element.clone());
            }
        }
        scene
    }

    fn export_dialog_background(&self) -> &str {
        if self.export_with_background {
            self.export_background()
        } else {
            "transparent"
        }
    }

    /// Rasterize the scene once so the dialog can show it without re-exporting
    /// on every frame.
    fn refresh_export_preview(&mut self) {
        let scene = self.export_scene();
        let background = self.export_dialog_background().to_string();
        self.export_preview = crate::render::export::export_png_with_options(
            &scene,
            &background,
            1.0,
            &self.document.files,
            self.export_dark_mode,
        )
        .ok()
        .and_then(|bytes| crate::bitmap::decode_render_image(&bytes, "image/png"));
    }

    fn export_dialog_png_bytes(&self) -> Result<Vec<u8>, String> {
        let scene = self.export_scene();
        let background = self.export_dialog_background().to_string();
        let mut bytes = crate::render::export::export_png_with_options(
            &scene,
            &background,
            self.export_scale,
            &self.document.files,
            self.export_dark_mode,
        )?;
        if self.export_embed_scene {
            let json = crate::export::scene_to_json(&scene, &self.app_state())
                .map_err(|e| e.to_string())?;
            bytes = crate::render::export::embed_png_scene_data(&bytes, &json);
        }
        Ok(bytes)
    }

    fn export_dialog_svg_string(&self) -> String {
        let scene = self.export_scene();
        let background = self.export_dialog_background().to_string();
        let mut svg = crate::export::export_svg_with_dark_mode(
            &scene,
            &background,
            &self.document.files,
            self.export_dark_mode,
        );
        if self.export_embed_scene
            && let Ok(json) = crate::export::scene_to_json(&scene, &self.app_state())
        {
            svg = crate::render::export::embed_svg_scene_data(&svg, &json);
        }
        svg
    }

    fn export_dialog_file_name(&self, extension: &str) -> String {
        let name = self.project_name_value();
        let base = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&name);
        format!("{base}.{extension}")
    }

    fn export_png_picker(&mut self, cx: &mut Context<Self>) {
        let name = self.export_dialog_file_name("png");
        let directory = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(path))) => {
                let path = path.to_string_lossy().to_string();
                let _ = this.update(cx, |editor, cx| {
                    match editor.export_dialog_png_bytes() {
                        Ok(bytes) => match std::fs::write(&path, bytes) {
                            Ok(()) => {
                                editor.status = Some(format!("png: {path}"));
                                editor.close_image_dialog();
                            }
                            Err(err) => editor.status = Some(err.to_string()),
                        },
                        Err(err) => editor.status = Some(err),
                    }
                    cx.notify();
                });
            }
            Ok(Err(_)) => {
                let _ = this.update(cx, |editor, cx| {
                    editor.status = Some(editor.i18n.t("status.unavailable"));
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    fn export_svg_picker(&mut self, cx: &mut Context<Self>) {
        let name = self.export_dialog_file_name("svg");
        let directory = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(path))) => {
                let path = path.to_string_lossy().to_string();
                let _ = this.update(cx, |editor, cx| {
                    let svg = editor.export_dialog_svg_string();
                    match std::fs::write(&path, svg) {
                        Ok(()) => {
                            editor.status = Some(format!("svg: {path}"));
                            editor.close_image_dialog();
                        }
                        Err(err) => editor.status = Some(err.to_string()),
                    }
                    cx.notify();
                });
            }
            Ok(Err(_)) => {
                let _ = this.update(cx, |editor, cx| {
                    editor.status = Some(editor.i18n.t("status.unavailable"));
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    fn export_dialog_copy_png(&mut self, cx: &App) {
        match self.export_dialog_png_bytes() {
            Ok(bytes) => {
                // The clipboard takes the encoded PNG plus a unique id so the
                // receiver can tell consecutive copies apart.
                let image = Image {
                    format: ImageFormat::Png,
                    bytes,
                    id: CLIPBOARD_IMAGE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                };
                cx.write_to_clipboard(ClipboardItem::new_image(&image));
                self.status = Some("png: clipboard".to_string());
            }
            Err(err) => self.status = Some(err),
        }
    }

    fn export_dialog_copy_svg(&mut self, cx: &App) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.export_dialog_svg_string()));
        self.status = Some("svg: clipboard".to_string());
    }

    fn handle_export_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        if key == "escape" {
            self.close_image_dialog();
            return true;
        }
        self.handle_name_field_key(key, key_char, modifiers)
    }

    pub fn export_png_bytes(&self, scale: f32) -> Result<Vec<u8>, String> {
        crate::export::export_png_with_files(
            &self.document.scene,
            self.export_background(),
            scale,
            &self.document.files,
        )
    }

    pub fn export_png_to(&mut self, path: &str) -> Result<(), String> {
        match self.export_png_bytes(2.0) {
            Ok(bytes) => {
                std::fs::write(path, bytes).map_err(|e| e.to_string())?;
                self.status = Some(format!("png: {path}"));
                Ok(())
            }
            Err(err) => {
                self.status = Some(err.clone());
                Err(err)
            }
        }
    }

    pub fn export_svg_to(&mut self, path: &str) -> Result<(), String> {
        std::fs::write(path, self.export_svg_string()).map_err(|e| e.to_string())?;
        self.status = Some(format!("svg: {path}"));
        Ok(())
    }

    fn on_new_document(&mut self) {
        self.push_checkpoint();
        self.document.scene = crate::core::scene::Scene::new();
        self.document.selected.clear();
    }

    /// Upstream gates the canvas reset behind a confirmation dialog, so every
    /// entry point opens the dialog and only the confirm button resets.
    fn confirm_clear_canvas(&mut self) {
        self.show_clear_dialog = false;
        self.on_new_document();
    }

    pub fn handle_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
        viewport: (f64, f64),
    ) -> bool {
        self.viewport = viewport;

        if self.eye_dropper.is_some() && key == "escape" {
            self.cancel_eye_dropper();
            return true;
        }

        if self.show_clear_dialog {
            if key == "escape" {
                self.show_clear_dialog = false;
            } else if key == "enter" || key == "numpadenter" {
                self.confirm_clear_canvas();
            }
            return true;
        }
        if self.show_save_dialog {
            return self.handle_save_key(key, key_char, modifiers);
        }
        if self.show_image_dialog {
            return self.handle_export_key(key, key_char, modifiers);
        }
        if self.show_mermaid_dialog {
            return self.handle_mermaid_key(key, key_char, modifiers);
        }
        if self.show_link_dialog && self.link_editing {
            return self.handle_link_key(key, key_char, modifiers);
        }
        if self.show_link_dialog && key == "escape" {
            self.cancel_link_dialog();
            return true;
        }
        if self.color_popup.is_some() {
            return self.handle_color_key(key, key_char, modifiers);
        }
        if self.font_popup {
            return self.handle_font_key(key, key_char, modifiers);
        }

        if self.show_palette {
            return self.handle_palette_key(key, key_char, modifiers);
        }
        if self.show_find {
            return self.handle_find_key(key, key_char, modifiers);
        }
        if self.show_help {
            if key == "escape" || key == "?" {
                self.show_help = false;
            }
            return true;
        }

        if key == "space" {
            self.space_pan = true;
            return true;
        }
        if key == "escape" {
            if self.editing_text.is_some() {
                self.commit_text_edit();
                return true;
            }
            if !self.lasso_path.is_empty() {
                self.lasso_path.clear();
                self.lasso_base_selection.clear();
                self.lasso_keep_selection = false;
                return true;
            }
            if !self.pending_points.is_empty() {
                self.pending_points.clear();
                self.cursor_world = None;
                return true;
            }
            self.context_menu = None;
            self.show_menu = false;
            self.show_more_tools = false;
            return true;
        }

        if let Some(id) = self.editing_text.clone() {
            return self.handle_text_key(&id, key, key_char, modifiers);
        }

        if modifiers.control || modifiers.platform {
            match key {
                "z" => {
                    if modifiers.shift {
                        self.on_redo();
                    } else {
                        self.on_undo();
                    }
                    return true;
                }
                "y" => {
                    self.on_redo();
                    return true;
                }
                "a" => {
                    self.document.select_all();
                    return true;
                }
                "c" => {
                    self.on_copy();
                    return true;
                }
                "x" => {
                    self.on_cut();
                    return true;
                }
                "v" => {
                    self.on_paste();
                    return true;
                }
                "d" => {
                    self.on_duplicate();
                    return true;
                }
                "g" => {
                    if modifiers.shift {
                        self.on_ungroup();
                    } else {
                        self.on_group();
                    }
                    return true;
                }

                "s" => {
                    self.pending_save_dialog = true;
                    return true;
                }

                "o" => {
                    self.pending_scene_open = true;
                    return true;
                }
                "delete" | "backspace" => {
                    self.show_clear_dialog = true;
                    return true;
                }
                "e" if modifiers.shift => {
                    self.pending_image_dialog = true;
                    return true;
                }
                "/" => {
                    self.toggle_palette();
                    return true;
                }
                "f" => {
                    self.open_find();
                    return true;
                }

                "'" => {
                    self.on_toggle_grid();
                    return true;
                }
                "0" => {
                    self.on_reset_zoom();
                    return true;
                }
                _ => {}
            }
        }

        if modifiers.shift {
            match key {
                "1" => {
                    self.document.zoom_to_fit(viewport, 40.0);
                    return true;
                }
                "2" => {
                    self.document.zoom_to_selection(viewport, 40.0);
                    return true;
                }
                _ => {}
            }
        }

        if key == "enter" || key == "numpadenter" {
            if !self.pending_points.is_empty() {
                self.finish_pending_line();
                return true;
            }
            return false;
        }

        if key == "delete" || key == "backspace" {
            self.on_delete();
            return true;
        }

        if modifiers.alt && !modifiers.control && !modifiers.platform {
            match key {
                "s" => {
                    self.document.snap_to_objects = !self.document.snap_to_objects;
                    return true;
                }
                "z" => {
                    self.document.zen_mode = !self.document.zen_mode;
                    return true;
                }
                "r" => {
                    self.document.view_mode = !self.document.view_mode;
                    return true;
                }
                "/" => {
                    self.document.show_stats = !self.document.show_stats;
                    return true;
                }
                _ => return false,
            }
        }

        if modifiers.alt {
            return false;
        }

        if key == "?" || (key == "/" && modifiers.shift) {
            self.open_help();
            return true;
        }

        match key {
            "v" | "1" => self.set_tool(Tool::Selection),
            "h" => self.set_tool(Tool::Hand),
            "r" | "2" => self.set_tool(Tool::Rectangle),
            "d" | "3" => self.set_tool(Tool::Diamond),
            "o" | "4" => self.set_tool(Tool::Ellipse),
            "a" | "5" => self.set_tool(Tool::Arrow),
            "l" | "6" => self.set_tool(Tool::Line),
            "p" | "7" => self.set_tool(Tool::FreeDraw),
            "t" | "8" => self.set_tool(Tool::Text),
            "i" | "9" => self.set_tool(Tool::Image),
            "f" => self.set_tool(Tool::Frame),
            "n" => self.set_tool(Tool::StickyNote),
            "e" | "0" => self.set_tool(Tool::Eraser),
            "b" => self.set_tool(Tool::BucketFill),
            "k" => self.set_tool(Tool::Laser),

            "q" => self.document.tool_locked = !self.document.tool_locked,
            _ => return false,
        }
        true
    }

    fn handle_text_key(
        &mut self,
        id: &str,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        match key {
            "escape" => {
                self.commit_text_edit();
                return true;
            }
            "enter" | "numpadenter" => {
                if let Some(crate::core::element::Element::Text(t)) =
                    self.document.scene.get_mut(id)
                {
                    t.text.push('\n');
                }
                self.refresh_text_bounds(id);
                return true;
            }
            "backspace" | "delete" => {
                if let Some(crate::core::element::Element::Text(t)) =
                    self.document.scene.get_mut(id)
                {
                    t.text.pop();
                }
                self.refresh_text_bounds(id);
                return true;
            }
            _ => {}
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return false;
        }
        let typed = match key_char {
            Some(c) if !c.is_empty() => Some(c),
            _ if key.len() == 1 => Some(key.to_string()),
            _ => None,
        };
        if let Some(ch) = typed {
            if self.replace_on_type {
                if let Some(crate::core::element::Element::Text(t)) =
                    self.document.scene.get_mut(id)
                {
                    t.text.clear();
                }
                self.replace_on_type = false;
            }
            if let Some(crate::core::element::Element::Text(t)) = self.document.scene.get_mut(id) {
                t.text.push_str(&ch);
            }
            self.refresh_text_bounds(id);
            return true;
        }
        false
    }

    fn handle_palette_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        match key {
            "escape" => {
                self.show_palette = false;
                return true;
            }
            "enter" | "numpadenter" => {
                self.run_palette_selection();
                return true;
            }
            "up" | "arrowup" => {
                self.palette_index = self.palette_index.saturating_sub(1);
                return true;
            }
            "down" | "arrowdown" => {
                if self.palette_index + 1 < self.palette_display().len() {
                    self.palette_index += 1;
                }
                return true;
            }
            "backspace" | "delete" => {
                self.palette_query.pop();
                self.palette_index = 0;
                return true;
            }
            "space" => {
                self.palette_query.push(' ');
                self.palette_index = 0;
                return true;
            }
            _ => {}
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return true;
        }
        if let Some(ch) = printable_char(key, key_char) {
            self.palette_query.push_str(&ch);
            self.palette_index = 0;
        }
        true
    }

    fn handle_find_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        match key {
            "escape" => {
                self.show_find = false;
                return true;
            }
            "enter" | "numpadenter" => {
                self.step_find(!modifiers.shift);
                return true;
            }
            "backspace" | "delete" => {
                self.find_query.pop();
                self.recompute_find();
                return true;
            }
            "space" => {
                self.find_query.push(' ');
                self.recompute_find();
                return true;
            }
            _ => {}
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return true;
        }
        if let Some(ch) = printable_char(key, key_char) {
            self.find_query.push_str(&ch);
            self.recompute_find();
        }
        true
    }

    pub(crate) fn color_popup(&self) -> Option<ColorTarget> {
        self.color_popup
    }

    pub(crate) fn color_popup_color(&self, target: ColorTarget) -> String {
        if target == ColorTarget::CanvasBackground {
            return self.document.view_background_color.clone();
        }
        if target == ColorTarget::Background
            && self.document.tool == Tool::BucketFill
            && self.document.selected.is_empty()
        {
            return self.bucket_fill_color();
        }
        self.document
            .selected_elements()
            .first()
            .map(|element| match target {
                ColorTarget::Stroke => element.base().stroke_color.clone(),
                ColorTarget::Background => element.base().background_color.clone(),

                ColorTarget::CanvasBackground => String::new(),
            })
            .unwrap_or_else(|| "transparent".to_string())
    }

    pub(crate) fn active_shade(&self, target: ColorTarget) -> usize {
        match target {
            ColorTarget::Stroke => self.shade_stroke,
            ColorTarget::Background => self.shade_background,

            ColorTarget::CanvasBackground => 0,
        }
    }

    fn set_active_shade(&mut self, target: ColorTarget, shade: usize) {
        match target {
            ColorTarget::Stroke => self.shade_stroke = shade,
            ColorTarget::Background => self.shade_background = shade,
            ColorTarget::CanvasBackground => {}
        }
    }

    pub(crate) fn toggle_color_popup(&mut self, target: ColorTarget) {
        if self.color_popup == Some(target) {
            self.close_color_popup();
            return;
        }
        self.font_popup = false;
        self.show_link_dialog = false;
        self.color_popup = Some(target);

        let color = self.color_popup_color(target);
        let shade = crate::theme::color_position(&color)
            .and_then(|(_, shade)| shade)
            .unwrap_or(match target {
                ColorTarget::Stroke => crate::theme::DEFAULT_STROKE_SHADE,
                ColorTarget::Background => crate::theme::DEFAULT_BACKGROUND_SHADE,
                ColorTarget::CanvasBackground => 0,
            });
        self.set_active_shade(target, shade);
        self.hex_buffer = color.trim_start_matches('#').to_string();
        self.hex_focused = false;
    }

    pub(crate) fn close_color_popup(&mut self) {
        self.color_popup = None;
        self.hex_focused = false;
    }

    pub(crate) fn apply_color(&mut self, target: ColorTarget, color: &str) {
        match target {
            ColorTarget::Stroke => self.document.set_selected_stroke_color(color),
            ColorTarget::Background => self.set_background_color(color),
            ColorTarget::CanvasBackground => {
                self.document.view_background_color = color.to_string()
            }
        }
        self.hex_buffer = color.trim_start_matches('#').to_string();
    }

    /// Background colors picked while the bucket tool is active choose the
    /// paint color; with a selection they restyle the selected elements.
    pub(crate) fn set_background_color(&mut self, color: &str) {
        if self.document.tool == Tool::BucketFill && self.document.selected.is_empty() {
            self.document.style.background_color = color.to_string();
        } else {
            self.document.set_selected_background_color(color);
        }
    }

    pub(crate) fn set_shade(&mut self, target: ColorTarget, shade: usize) {
        self.set_active_shade(target, shade);
    }

    pub(crate) fn hex_buffer(&self) -> &str {
        &self.hex_buffer
    }

    pub(crate) fn is_hex_focused(&self) -> bool {
        self.hex_focused
    }

    pub(crate) fn set_hex_focused(&mut self, focused: bool) {
        self.hex_focused = focused;
    }

    pub(crate) fn is_font_popup_open(&self) -> bool {
        self.font_popup
    }

    pub(crate) fn toggle_font_popup(&mut self) {
        self.font_popup = !self.font_popup;
        if self.font_popup {
            self.color_popup = None;
            self.show_link_dialog = false;
            self.font_query.clear();
        }
    }

    pub(crate) fn font_search(&self) -> &str {
        &self.font_query
    }

    pub(crate) fn picker_font_groups(&self) -> (Vec<FontFamily>, Vec<FontFamily>) {
        let mut scene_families: Vec<FontFamily> = Vec::new();
        for element in self.document.scene.non_deleted() {
            if let Element::Text(text) = element
                && !scene_families.contains(&text.font_family)
            {
                scene_families.push(text.font_family);
            }
        }
        let query = self.font_query.to_lowercase();
        let matches = |family: &FontFamily| {
            query.is_empty()
                || crate::properties::font_label(*family)
                    .to_lowercase()
                    .contains(&query)
        };

        let scene: Vec<FontFamily> = scene_families.iter().copied().filter(matches).collect();
        let available: Vec<FontFamily> = crate::properties::all_fonts()
            .into_iter()
            .filter(|family| !scene_families.contains(family) && matches(family))
            .collect();
        (scene, available)
    }

    pub(crate) fn first_picker_font(&self) -> Option<FontFamily> {
        let (scene, available) = self.picker_font_groups();
        scene.into_iter().chain(available).next()
    }

    /// Opens the popup in its input mode, as upstream's action button and the
    /// post-draw flow do.
    pub(crate) fn open_link_dialog(&mut self) {
        self.link_input = self.selected_link().unwrap_or_default();
        self.show_link_dialog = true;
        self.link_editing = true;
        self.color_popup = None;
        self.font_popup = false;
    }

    /// Opens the popup in summary mode for a selected embeddable or linked
    /// element, matching upstream's `showHyperlinkPopup: "info"`.
    fn open_link_info(&mut self, link: Option<String>) {
        self.link_input = link.unwrap_or_default();
        self.show_link_dialog = true;
        self.link_editing = false;
        self.color_popup = None;
        self.font_popup = false;
    }

    pub(crate) fn begin_link_edit(&mut self) {
        self.link_input = self.selected_link().unwrap_or_default();
        self.link_editing = true;
    }

    pub fn is_link_dialog_open(&self) -> bool {
        self.show_link_dialog
    }

    pub fn is_link_editing(&self) -> bool {
        self.link_editing
    }

    /// Shows the summary popup when a pointer lands on an embeddable or a
    /// linked element, and dismisses it otherwise.
    fn sync_link_popup(&mut self, id: &str) {
        let Some(element) = self.document.scene.get(id) else {
            self.show_link_dialog = false;
            return;
        };
        let link = element.base().link.clone().filter(|link| !link.is_empty());
        if element.kind() == crate::core::element::ElementType::Embeddable || link.is_some() {
            self.open_link_info(link);
        } else {
            self.show_link_dialog = false;
        }
    }

    pub fn selected_link(&self) -> Option<String> {
        self.document
            .selected_elements()
            .first()
            .and_then(|element| element.base().link.clone())
    }

    pub fn link_input(&self) -> &str {
        &self.link_input
    }

    pub(crate) fn clear_link_input(&mut self) {
        self.link_input.clear();
    }

    /// Removes the link straight from the summary popup, which upstream does
    /// through its trash button without reopening the input.
    pub(crate) fn remove_link(&mut self) {
        self.document.set_selected_link(None);
        self.link_input.clear();
        self.show_link_dialog = true;
        self.link_editing = false;
    }

    pub(crate) fn cancel_link_dialog(&mut self) {
        self.show_link_dialog = false;
    }

    pub(crate) fn confirm_link(&mut self) {
        let link = self.link_input.trim().to_string();
        if link.is_empty() {
            self.document.set_selected_link(None);
        } else {
            self.document.set_selected_link(Some(&link));
        }
        // Submitting hands the popup back to its summary mode, as upstream's
        // `showHyperlinkPopup: "info"` does.
        self.link_input = link;
        self.link_editing = false;
    }

    fn handle_link_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        match key {
            "escape" => {
                self.cancel_link_dialog();
                return true;
            }
            "enter" | "numpadenter" => {
                self.confirm_link();
                return true;
            }
            "backspace" | "delete" => {
                self.link_input.pop();
                return true;
            }
            "space" => {
                self.link_input.push(' ');
                return true;
            }
            _ => {}
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return true;
        }
        if let Some(ch) = printable_char(key, key_char) {
            self.link_input.push_str(&ch);
        }
        true
    }

    /// The selected byte span of the filename field, normalized to
    /// `start <= end`. An empty span means there is a caret but no selection.
    fn save_name_range(&self) -> (usize, usize) {
        let len = self.project_name.len();
        let caret = self.name_caret.min(len);
        let anchor = self.name_anchor.min(len);
        (caret.min(anchor), caret.max(anchor))
    }

    fn save_name_select_all(&mut self) {
        self.name_anchor = 0;
        self.name_caret = self.project_name.len();
    }

    /// Replaces the selection, or inserts at the caret, then parks the caret
    /// after the inserted text and drops the selection.
    fn save_name_insert(&mut self, text: &str) {
        let (start, end) = self.save_name_range();
        self.project_name.replace_range(start..end, text);
        self.name_caret = start + text.len();
        self.name_anchor = self.name_caret;
    }

    fn save_name_backspace(&mut self) {
        let (start, end) = self.save_name_range();
        let at = if start != end {
            self.project_name.replace_range(start..end, "");
            start
        } else if start > 0 {
            let prev = char_boundary_before(&self.project_name, start);
            self.project_name.replace_range(prev..start, "");
            prev
        } else {
            return;
        };
        self.name_caret = at;
        self.name_anchor = at;
    }

    fn save_name_delete(&mut self) {
        let (start, end) = self.save_name_range();
        if start == end {
            let next = char_boundary_after(&self.project_name, start);
            if next == start {
                return;
            }
            self.project_name.replace_range(start..next, "");
        } else {
            self.project_name.replace_range(start..end, "");
        }
        self.name_caret = start;
        self.name_anchor = start;
    }

    /// Moves the caret. Without `shift` a live selection collapses to the
    /// corresponding edge first, which is how every text field behaves.
    fn save_name_move(&mut self, key: &str, shift: bool) {
        let len = self.project_name.len();
        let caret = self.name_caret.min(len);
        let (start, end) = self.save_name_range();
        let selected = start != end;

        let next = match key {
            "left" if !shift && selected => start,
            "left" => char_boundary_before(&self.project_name, caret),
            "right" if !shift && selected => end,
            "right" => char_boundary_after(&self.project_name, caret),
            "home" => 0,
            "end" => len,
            _ => caret,
        };

        self.name_caret = next;
        if !shift {
            self.name_anchor = next;
        }
    }

    fn save_name_copy(&self, cx: &App) {
        let (start, end) = self.save_name_range();
        if start == end {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.project_name[start..end].to_string(),
        ));
    }

    fn save_name_cut(&mut self, cx: &App) {
        let (start, end) = self.save_name_range();
        if start == end {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.project_name[start..end].to_string(),
        ));
        self.project_name.replace_range(start..end, "");
        self.name_caret = start;
        self.name_anchor = start;
    }

    fn save_name_paste(&mut self, cx: &App) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // The field is single line; drop line breaks the way a browser drops
        // them when pasting into `<input type="text">`.
        let text: String = text
            .chars()
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .collect();
        if !text.is_empty() {
            self.save_name_insert(&text);
        }
    }

    /// The selected byte span of the Mermaid source, normalized to
    /// `start <= end`. An empty span means there is a caret but no selection.
    fn mermaid_range(&self) -> (usize, usize) {
        let len = self.mermaid_text.len();
        let caret = self.mermaid_caret.min(len);
        let anchor = self.mermaid_anchor.min(len);
        (caret.min(anchor), caret.max(anchor))
    }

    fn mermaid_select_all(&mut self) {
        self.mermaid_anchor = 0;
        self.mermaid_caret = self.mermaid_text.len();
    }

    fn mermaid_insert(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\t', "  ");
        let (start, end) = self.mermaid_range();
        self.mermaid_text.replace_range(start..end, &text);
        self.mermaid_caret = start + text.len();
        self.mermaid_anchor = self.mermaid_caret;
        self.refresh_mermaid_preview();
    }

    fn mermaid_backspace(&mut self) {
        let (start, end) = self.mermaid_range();
        let at = if start != end {
            self.mermaid_text.replace_range(start..end, "");
            start
        } else if start > 0 {
            let prev = char_boundary_before(&self.mermaid_text, start);
            self.mermaid_text.replace_range(prev..start, "");
            prev
        } else {
            return;
        };
        self.mermaid_caret = at;
        self.mermaid_anchor = at;
        self.refresh_mermaid_preview();
    }

    fn mermaid_delete(&mut self) {
        let (start, end) = self.mermaid_range();
        if start == end {
            let next = char_boundary_after(&self.mermaid_text, start);
            if next == start {
                return;
            }
            self.mermaid_text.replace_range(start..next, "");
        } else {
            self.mermaid_text.replace_range(start..end, "");
        }
        self.mermaid_caret = start;
        self.mermaid_anchor = start;
        self.refresh_mermaid_preview();
    }

    fn mermaid_move(&mut self, key: &str, shift: bool) {
        if key == "up" || key == "down" {
            self.mermaid_move_vertical(key == "up", shift);
            return;
        }

        let len = self.mermaid_text.len();
        let caret = self.mermaid_caret.min(len);
        let (start, end) = self.mermaid_range();
        let selected = start != end;

        let next = match key {
            "left" if !shift && selected => start,
            "left" => char_boundary_before(&self.mermaid_text, caret),
            "right" if !shift && selected => end,
            "right" => char_boundary_after(&self.mermaid_text, caret),
            "home" => mermaid_line_start(&self.mermaid_text, caret),
            "end" => mermaid_line_end(&self.mermaid_text, caret),
            _ => caret,
        };

        self.mermaid_caret = next;
        if !shift {
            self.mermaid_anchor = next;
        }
    }

    /// Moves the caret a line up or down while keeping the column, which is how
    /// a code editor's arrow keys behave.
    fn mermaid_move_vertical(&mut self, up: bool, shift: bool) {
        let len = self.mermaid_text.len();
        let (start, end) = self.mermaid_range();
        let selected = start != end;
        let mut caret = self.mermaid_caret.min(len);
        if !shift && selected {
            caret = if up { start } else { end };
        }

        let line_start = mermaid_line_start(&self.mermaid_text, caret);
        let column = caret - line_start;
        let target = if up {
            if line_start == 0 {
                Some(0)
            } else {
                let previous_end = line_start - 1;
                let previous_start = mermaid_line_start(&self.mermaid_text, previous_end);
                Some(previous_start + column.min(previous_end - previous_start))
            }
        } else {
            let line_end = mermaid_line_end(&self.mermaid_text, caret);
            if line_end == len {
                None
            } else {
                let next_start = line_end + 1;
                let next_end = mermaid_line_end(&self.mermaid_text, next_start);
                Some(next_start + column.min(next_end - next_start))
            }
        };

        let Some(mut target) = target else {
            return;
        };
        while target > 0 && !self.mermaid_text.is_char_boundary(target) {
            target -= 1;
        }
        self.mermaid_caret = target;
        if !shift {
            self.mermaid_anchor = target;
        }
    }

    fn mermaid_copy(&self, cx: &App) {
        let (start, end) = self.mermaid_range();
        if start == end {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.mermaid_text[start..end].to_string(),
        ));
    }

    fn mermaid_cut(&mut self, cx: &App) {
        let (start, end) = self.mermaid_range();
        if start == end {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.mermaid_text[start..end].to_string(),
        ));
        self.mermaid_text.replace_range(start..end, "");
        self.mermaid_caret = start;
        self.mermaid_anchor = start;
        self.refresh_mermaid_preview();
    }

    fn mermaid_paste(&mut self, cx: &App) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if !text.is_empty() {
            self.mermaid_insert(&text);
        }
    }

    fn handle_mermaid_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        if key == "escape" {
            self.close_mermaid_dialog();
            return true;
        }
        if (modifiers.control || modifiers.platform) && (key == "enter" || key == "numpadenter") {
            self.insert_mermaid_diagram();
            return true;
        }
        if modifiers.control || modifiers.platform {
            // c/x/v are routed through `on_key_down`, where the clipboard is
            // available; the rest act on the source buffer directly.
            match key {
                "a" => self.mermaid_select_all(),
                "backspace" => self.mermaid_backspace(),
                "delete" => self.mermaid_delete(),
                "left" | "right" | "home" | "end" | "up" | "down" => {
                    self.mermaid_move(key, modifiers.shift)
                }
                _ => {}
            }
            return true;
        }
        if modifiers.alt {
            return true;
        }

        match key {
            "backspace" => self.mermaid_backspace(),
            "delete" => self.mermaid_delete(),
            "enter" | "numpadenter" => self.mermaid_insert("\n"),
            "tab" => self.mermaid_insert("  "),
            "left" | "right" | "home" | "end" | "up" | "down" => {
                self.mermaid_move(key, modifiers.shift)
            }
            "space" => self.mermaid_insert(" "),
            _ => {
                if let Some(ch) = printable_char(key, key_char) {
                    self.mermaid_insert(&ch);
                }
            }
        }
        true
    }

    fn handle_save_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        if key == "escape" {
            self.close_save_dialog();
            return true;
        }

        self.handle_name_field_key(key, key_char, modifiers)
    }

    /// Edits the shared file-name field used by both the save and the export
    /// dialogs: caret movement, selection, deletion and character input.
    fn handle_name_field_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        if modifiers.control || modifiers.platform {
            // c/x/v are routed through `on_key_down`, where the clipboard is
            // available; the rest have no meaning in a single-line field.
            match key {
                "a" => self.save_name_select_all(),
                "backspace" => self.save_name_backspace(),
                "delete" => self.save_name_delete(),
                "left" | "right" | "home" | "end" => self.save_name_move(key, modifiers.shift),
                _ => {}
            }
            return true;
        }
        if modifiers.alt {
            return true;
        }

        match key {
            "backspace" => self.save_name_backspace(),
            "delete" => self.save_name_delete(),
            "left" | "right" | "home" | "end" => self.save_name_move(key, modifiers.shift),
            "space" => self.save_name_insert(" "),
            _ => {
                if let Some(ch) = printable_char(key, key_char) {
                    self.save_name_insert(&ch);
                }
            }
        }
        true
    }

    fn handle_color_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        let Some(target) = self.color_popup else {
            return false;
        };
        if key == "escape" {
            if self.hex_focused {
                self.hex_focused = false;
            } else {
                self.close_color_popup();
            }
            return true;
        }
        if key == "tab" {
            self.hex_focused = !self.hex_focused;
            return true;
        }

        if self.hex_focused {
            match key {
                "enter" | "numpadenter" => {
                    self.hex_focused = false;
                    return true;
                }
                "backspace" | "delete" => {
                    self.hex_buffer.pop();
                    self.apply_hex_buffer(target);
                    return true;
                }
                _ => {}
            }
            if modifiers.control || modifiers.platform || modifiers.alt {
                return true;
            }
            if let Some(ch) = printable_char(key, key_char)
                && ch.chars().all(|c| c.is_ascii_hexdigit())
            {
                self.hex_buffer.push_str(&ch.to_lowercase());
                self.apply_hex_buffer(target);
            }
            return true;
        }

        if target == ColorTarget::CanvasBackground {
            return true;
        }

        if modifiers.control || modifiers.platform || modifiers.alt {
            return true;
        }

        if modifiers.shift
            && let Some(index) = shade_digit(key)
        {
            self.set_active_shade(target, index);
            return true;
        }
        if let Some(index) = shade_digit(key) {
            let custom = self.custom_colors(target);
            if let Some(color) = custom.get(index).cloned() {
                self.apply_color(target, &color);
            }
            return true;
        }
        if key.len() == 1 {
            let typed = key.to_lowercase();
            if let Some(position) = PALETTE_HOTKEYS
                .iter()
                .position(|hotkey| *hotkey == typed.as_str())
            {
                let entry = &crate::theme::element_palette()[position];
                let color = entry.at(self.active_shade(target));
                self.apply_color(target, color);
                return true;
            }
        }

        let color = self.color_popup_color(target);
        let palette = crate::theme::element_palette();
        let current = crate::theme::color_position(&color)
            .and_then(|(name, _)| palette.iter().position(|entry| entry.name == name))
            .unwrap_or(0);
        if let Some(next) = palette_step(key, current, palette.len()) {
            self.apply_color(target, palette[next].at(self.active_shade(target)));
        }
        true
    }

    pub(crate) fn custom_colors(&self, target: ColorTarget) -> Vec<String> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for element in self.document.scene.non_deleted() {
            let color = match target {
                ColorTarget::Stroke => element.base().stroke_color.as_str(),
                ColorTarget::Background => element.base().background_color.as_str(),

                ColorTarget::CanvasBackground => continue,
            };
            if color == "transparent" || !crate::theme::is_custom_color(color) {
                continue;
            }
            match counts.iter_mut().find(|(existing, _)| existing == color) {
                Some((_, count)) => *count += 1,
                None => counts.push((color.to_string(), 1)),
            }
        }

        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        counts.into_iter().map(|(color, _)| color).take(5).collect()
    }

    fn apply_hex_buffer(&mut self, target: ColorTarget) {
        if self.hex_buffer.len() == 6 && self.hex_buffer.chars().all(|c| c.is_ascii_hexdigit()) {
            let color = format!("#{}", self.hex_buffer.to_lowercase());
            self.apply_color(target, &color);
        }
    }

    fn handle_font_key(
        &mut self,
        key: &str,
        key_char: Option<String>,
        modifiers: Modifiers,
    ) -> bool {
        match key {
            "escape" => {
                self.font_popup = false;
                return true;
            }
            "enter" | "numpadenter" => {
                if let Some(family) = self.first_picker_font() {
                    self.document.set_selected_font_family(family);
                }
                return true;
            }
            "backspace" | "delete" => {
                self.font_query.pop();
                return true;
            }
            "space" => {
                self.font_query.push(' ');
                return true;
            }
            _ => {}
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return true;
        }
        if let Some(ch) = printable_char(key, key_char) {
            self.font_query.push_str(&ch);
        }
        true
    }
}

const PALETTE_HOTKEYS: [&str; 15] = [
    "q", "w", "e", "r", "t", "a", "s", "d", "f", "g", "z", "x", "c", "v", "b",
];

fn shade_digit(key: &str) -> Option<usize> {
    match key {
        "1" => Some(0),
        "2" => Some(1),
        "3" => Some(2),
        "4" => Some(3),
        "5" => Some(4),
        _ => None,
    }
}

fn palette_step(key: &str, index: usize, length: usize) -> Option<usize> {
    const COLUMNS: usize = 5;
    let rows = length.div_ceil(COLUMNS);
    match key {
        "left" | "arrowleft" => Some(if index == 0 { length - 1 } else { index - 1 }),
        "right" | "arrowright" => Some((index + 1) % length),
        "down" | "arrowdown" => {
            let next = index + COLUMNS;
            if next >= length {
                Some(index % COLUMNS)
            } else {
                Some(next)
            }
        }
        "up" | "arrowup" => {
            let previous = index as isize - COLUMNS as isize;
            let candidate = if previous < 0 {
                (COLUMNS * rows) as isize + previous
            } else {
                previous
            };
            if candidate < 0 || candidate as usize >= length {
                None
            } else {
                Some(candidate as usize)
            }
        }
        _ => None,
    }
}

fn printable_char(key: &str, key_char: Option<String>) -> Option<String> {
    match key_char {
        Some(c) if !c.is_empty() => Some(c),
        _ if key.chars().count() == 1 => Some(key.to_string()),
        _ => None,
    }
}

/// The byte offset of the `char` boundary immediately before `index`, so caret
/// movement never lands inside a multi-byte character.
fn char_boundary_before(text: &str, index: usize) -> usize {
    text[..index.min(text.len())]
        .char_indices()
        .next_back()
        .map(|(offset, _)| offset)
        .unwrap_or(0)
}

/// The byte offset of the `char` boundary immediately after `index`.
fn char_boundary_after(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    match text[index..].chars().next() {
        Some(ch) => index + ch.len_utf8(),
        None => index,
    }
}

/// Start of the line containing `index`.
fn mermaid_line_start(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text[..index].rfind('\n').map_or(0, |offset| offset + 1)
}

/// End of the line containing `index`, excluding the newline itself.
fn mermaid_line_end(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    text[index..]
        .find('\n')
        .map_or(text.len(), |offset| index + offset)
}

/// Splits the upstream description markup (`<flowchartLink>…</flowchartLink>`)
/// into plain text plus the byte ranges that should render as links.
fn mermaid_description_runs(text: &str) -> (String, Vec<std::ops::Range<usize>>) {
    const TAGS: [(&str, &str); 4] = [
        ("<flowchartLink>", "</flowchartLink>"),
        ("<sequenceLink>", "</sequenceLink>"),
        ("<classLink>", "</classLink>"),
        ("<erdLink>", "</erdLink>"),
    ];

    let mut plain = String::new();
    let mut links = Vec::new();
    let mut rest = text;
    let mut link_start: Option<usize> = None;
    loop {
        let Some(open) = rest.find('<') else {
            plain.push_str(rest);
            break;
        };
        plain.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else {
            plain.push_str(rest);
            break;
        };
        let tag = &rest[..=close];
        rest = &rest[close + 1..];

        if let Some((_, closing)) = TAGS.iter().find(|(opening, _)| *opening == tag) {
            link_start = Some(plain.len());
            let _ = closing;
        } else if let Some((opening, _)) = TAGS.iter().find(|(_, closing)| *closing == tag) {
            let _ = opening;
            if let Some(start) = link_start.take()
                && plain.len() > start
            {
                links.push(start..plain.len());
            }
        } else {
            plain.push_str(tag);
        }
    }
    (plain, links)
}

/// Code font for the Mermaid editor. Upstream relies on the browser's
/// `monospace`; GPUI's generic `monospace` maps to a ligature-capable family
/// here, which would draw `-->` as a single arrow, so name the font that
/// fontconfig resolves for `monospace` instead.
const MERMAID_CODE_FONT: &str = "Noto Sans Mono";

/// Colours for the Mermaid source editor, mirroring `CodeMirrorEditor.tsx`
/// (VS light theme in light mode, VS dark theme in dark mode).
#[derive(Clone, Copy)]
struct MermaidCodeColors {
    /// Editor surface: `#fff` in light, `#1e1e1e` in dark.
    background: Rgba,
    /// Default token and caret colour.
    text: Rgba,
    caret: Rgba,
    gutter: Rgba,
    keyword: Rgba,
    string: Rgba,
    comment: Rgba,
    number: Rgba,
    operator: Rgba,
    punctuation: Rgba,
    variable: Rgba,
    bracket: Rgba,
}

impl MermaidCodeColors {
    fn for_dark(dark: bool) -> Self {
        if dark {
            Self {
                background: rgb(0x1e1e1e),
                text: rgb(0xd4d4d4),
                caret: rgb(0xffffff),
                gutter: rgb(0x858585),
                keyword: rgb(0x569cd6),
                string: rgb(0xce9178),
                comment: rgb(0x6a9955),
                number: rgb(0xb5cea8),
                operator: rgb(0xd4d4d4),
                punctuation: rgb(0xd4d4d4),
                variable: rgb(0x9cdcfe),
                bracket: rgb(0xffd700),
            }
        } else {
            Self {
                background: rgb(0xffffff),
                text: rgb(0x1e1e1e),
                caret: rgb(0x000000),
                gutter: rgb(0x999999),
                keyword: rgb(0x0000ff),
                string: rgb(0xa31515),
                comment: rgb(0x008000),
                number: rgb(0x098658),
                operator: rgb(0x1e1e1e),
                punctuation: rgb(0x1e1e1e),
                variable: rgb(0x001080),
                bracket: rgb(0xaf00db),
            }
        }
    }
}

/// Diagram declarations, matched case-insensitively like upstream's regex.
const MERMAID_DIAGRAM_KEYWORDS: [&str; 15] = [
    "flowchart",
    "graph",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "erDiagram",
    "gantt",
    "pie",
    "mindmap",
    "journey",
    "gitGraph",
    "timeline",
    "quadrantChart",
    "sankey",
    "xychart",
];

/// Flow directions, case-sensitive upstream (`TB|TD|BT|RL|LR`).
const MERMAID_DIRECTIONS: [&str; 5] = ["TB", "TD", "BT", "RL", "LR"];

/// Block keywords, matched case-insensitively.
const MERMAID_KEYWORDS: [&str; 23] = [
    "subgraph",
    "end",
    "participant",
    "actor",
    "loop",
    "alt",
    "else",
    "opt",
    "par",
    "critical",
    "break",
    "rect",
    "note",
    "over",
    "activate",
    "deactivate",
    "title",
    "section",
    "class",
    "style",
    "linkStyle",
    "classDef",
    "click",
];

/// Matches `keyword\b` at the start of `rest`, returning the keyword length.
fn mermaid_keyword_len(rest: &str, keywords: &[&str], case_insensitive: bool) -> Option<usize> {
    keywords.iter().find_map(|keyword| {
        let len = keyword.len();
        if rest.len() < len || !rest.is_char_boundary(len) {
            return None;
        }
        let head = &rest[..len];
        let matches = if case_insensitive {
            head.eq_ignore_ascii_case(keyword)
        } else {
            head == *keyword
        };
        if !matches {
            return None;
        }
        // Upstream's `\b` requires a non-word character after the keyword.
        let boundary = rest[len..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_ascii_alphanumeric() || next == '_'));
        boundary.then_some(len)
    })
}

/// Mirrors upstream's operator alternation:
/// `[-.=<>|ox]+>`, then `<[-.=<>|ox]+`, then `--+|\.\.+|==+`.
fn mermaid_operator_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let arrow_class =
        |byte: u8| matches!(byte, b'-' | b'.' | b'=' | b'<' | b'>' | b'|' | b'o' | b'x');

    let mut run = 0;
    while run < bytes.len() && arrow_class(bytes[run]) {
        run += 1;
    }
    for len in (1..=run).rev() {
        if bytes.get(len) == Some(&b'>') {
            return Some(len + 1);
        }
    }

    if bytes.first() == Some(&b'<') {
        let mut len = 1;
        while len < bytes.len() && arrow_class(bytes[len]) {
            len += 1;
        }
        if len > 1 {
            return Some(len);
        }
    }

    if rest.starts_with("--") {
        return Some(rest.chars().take_while(|ch| *ch == '-').count());
    }
    if rest.starts_with("..") {
        return Some(rest.chars().take_while(|ch| *ch == '.').count());
    }
    if rest.starts_with("==") {
        return Some(rest.chars().take_while(|ch| *ch == '=').count());
    }
    None
}

/// Tokenises one line with the same rules as upstream's `mermaid-lang-lite.ts`
/// stream parser, so the editor's colours match excalidraw.com.
fn mermaid_token_runs(
    line: &str,
    colors: &MermaidCodeColors,
) -> Vec<(std::ops::Range<usize>, Rgba)> {
    let mut runs = Vec::new();
    let mut index = 0;
    while index < line.len() {
        let rest = &line[index..];
        let advance;

        if rest.starts_with("%%") {
            runs.push((index..line.len(), colors.comment));
            break;
        }

        if rest.starts_with('"') {
            // A double-quoted string, honouring backslash escapes.
            let mut chars = rest.char_indices();
            chars.next();
            let mut escaped = false;
            let mut closing = None;
            for (offset, ch) in chars {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    closing = Some(offset + ch.len_utf8());
                    break;
                }
            }
            if let Some(end) = closing {
                runs.push((index..index + end, colors.string));
                index += end;
                continue;
            }
        }

        if let Some(len) = mermaid_keyword_len(rest, &MERMAID_DIAGRAM_KEYWORDS, true)
            .or_else(|| mermaid_keyword_len(rest, &MERMAID_DIRECTIONS, false))
            .or_else(|| mermaid_keyword_len(rest, &MERMAID_KEYWORDS, true))
        {
            runs.push((index..index + len, colors.keyword));
            advance = len;
        } else if let Some(len) = mermaid_operator_len(rest) {
            runs.push((index..index + len, colors.operator));
            advance = len;
        } else {
            let ch = rest.chars().next().unwrap_or(' ');
            let color = if "[](){}|<>".contains(ch) {
                Some(colors.bracket)
            } else if ch.is_ascii_alphabetic() || ch == '_' {
                let len = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                runs.push((index..index + len, colors.variable));
                index += len;
                continue;
            } else if ch.is_ascii_digit() {
                let mut len = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                if rest[len..].starts_with('.')
                    && rest[len + 1..].starts_with(|c: char| c.is_ascii_digit())
                {
                    len += 1 + rest[len + 1..]
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(rest.len() - len - 1);
                }
                runs.push((index..index + len, colors.number));
                index += len;
                continue;
            } else if ",:;".contains(ch) {
                Some(colors.punctuation)
            } else {
                None
            };
            if let Some(color) = color {
                runs.push((index..index + ch.len_utf8(), color));
            }
            advance = ch.len_utf8();
        }

        index += advance;
    }
    runs
}

/// Applies upstream's dark-mode export filter to a single `#rrggbb` colour.
/// Transparent stays transparent, matching `exportToCanvas` in dark theme.
fn darken_hex(color: &str) -> String {
    if color.eq_ignore_ascii_case("transparent") {
        return color.to_string();
    }
    crate::render::export::dark_mode_hex(color)
}

fn darken_element_colors(elements: &mut [Element]) {
    for element in elements {
        let base = element.base_mut();
        base.stroke_color = darken_hex(&base.stroke_color);
        base.background_color = darken_hex(&base.background_color);
    }
}

fn element_search_text(element: &Element) -> Option<&str> {
    match element {
        Element::Text(t) => Some(t.text.as_str()),
        _ => None,
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

const TOOL_ROW: &[(Tool, &str, Option<&str>)] = &[
    (Tool::Hand, "handIcon", None),
    (Tool::Selection, "SelectionIcon", Some("V")),
    (Tool::Rectangle, "RectangleIcon", Some("R")),
    (Tool::Diamond, "DiamondIcon", Some("D")),
    (Tool::Ellipse, "EllipseIcon", Some("O")),
    (Tool::Arrow, "ArrowIcon", Some("A")),
    (Tool::Line, "LineIcon", Some("L")),
    (Tool::FreeDraw, "FreedrawIcon", Some("P")),
    (Tool::Text, "TextIcon", Some("T")),
    (Tool::StickyNote, "stickyNoteToolIcon", Some("N")),
    (Tool::Eraser, "EraserIcon", Some("E")),
];

fn extra_tools_trigger(tool: Tool) -> (&'static str, bool) {
    match tool {
        Tool::Image => ("ImageIcon", true),
        Tool::Frame => ("frameToolIcon", true),
        Tool::Laser => ("laserPointerToolIcon", true),
        Tool::BucketFill => ("bucketFillIcon", true),
        Tool::Lasso => ("lassoToolIcon", true),
        Tool::Embeddable => ("EmbedIcon", true),
        _ => ("DotsIcon", false),
    }
}

struct MoreTool {
    key: &'static str,
    icon: &'static str,
    shortcut: Option<&'static str>,

    ai: bool,
    action: MoreToolAction,
}

enum MoreToolAction {
    Tool(Tool),

    Mermaid,

    Stub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MermaidTab {
    TextToDiagram,
    Mermaid,
}

const MERMAID_EXAMPLE: &str = "\
flowchart TD
 A[Christmas] -->|Get money| B(Go shopping)
 B --> C{Let me think}
 C -->|One| D[Laptop]
 C -->|Two| E[iPhone]
 C -->|Three| F[Car]
";

const MORE_TOOLS: &[MoreTool] = &[
    MoreTool {
        key: "toolbar.image",
        icon: "ImageIcon",
        shortcut: Some("9"),
        ai: false,
        action: MoreToolAction::Tool(Tool::Image),
    },
    MoreTool {
        key: "toolbar.frame",
        icon: "frameToolIcon",
        shortcut: Some("F"),
        ai: false,
        action: MoreToolAction::Tool(Tool::Frame),
    },
    MoreTool {
        key: "toolbar.embed",
        icon: "EmbedIcon",
        shortcut: None,
        ai: false,
        action: MoreToolAction::Tool(Tool::Embeddable),
    },
    MoreTool {
        key: "toolbar.drawToShape",
        icon: "drawShapeToolIcon",
        shortcut: Some("Shift+X"),
        ai: false,
        action: MoreToolAction::Stub,
    },
    MoreTool {
        key: "toolbar.laser",
        icon: "laserPointerToolIcon",
        shortcut: Some("K"),
        ai: false,
        action: MoreToolAction::Tool(Tool::Laser),
    },
    MoreTool {
        key: "toolbar.bucketFill",
        icon: "bucketFillIcon",
        shortcut: Some("B"),
        ai: false,
        action: MoreToolAction::Tool(Tool::BucketFill),
    },
    MoreTool {
        key: "toolbar.lasso",
        icon: "lassoToolIcon",
        shortcut: None,
        ai: false,
        action: MoreToolAction::Tool(Tool::Lasso),
    },
    MoreTool {
        key: "toolbar.textToDiagram",
        icon: "textToDiagramIcon",
        shortcut: None,
        ai: true,
        action: MoreToolAction::Stub,
    },
    MoreTool {
        key: "toolbar.mermaid",
        icon: "mermaidIcon",
        shortcut: None,
        ai: false,
        action: MoreToolAction::Mermaid,
    },
    MoreTool {
        key: "toolbar.wireframeToCode",
        icon: "wireframeToCodeIcon",
        shortcut: None,
        ai: true,
        action: MoreToolAction::Stub,
    },
];

#[derive(Clone, Copy)]
enum CanvasButton {
    ConvertToCode,

    CopySource,

    Fullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropKind {
    Scene,

    Library,

    Unknown,
}

#[derive(Clone, Copy, Debug)]
struct FileDragState {
    kind: DropKind,

    shift: bool,
}

impl FileDragState {
    fn keeps_content(self) -> bool {
        self.shift || self.kind == DropKind::Library
    }
}

#[derive(Clone, Debug)]
struct CursorHintState {
    icon: &'static str,

    nonce: u64,
}

#[derive(Clone, Debug)]
struct ToastState {
    message: String,
    closable: bool,

    nonce: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorTarget {
    Stroke,
    Background,
    CanvasBackground,
}

#[derive(Clone, Debug)]
struct EyeDropperState {
    target: ColorTarget,

    at: (f64, f64),

    color: String,
}

fn iframe_generation_finished(base: &crate::core::element::ElementBase) -> bool {
    base.custom_data
        .as_ref()
        .and_then(|data| data.get("generationData"))
        .and_then(|generation| generation.get("status"))
        .and_then(|status| status.as_str())
        == Some("done")
}

fn locked_subject_id(element: &Element) -> String {
    element
        .base()
        .group_ids
        .last()
        .cloned()
        .unwrap_or_else(|| element.id().to_string())
}

fn palette_of(theme: Theme) -> crate::design::Palette {
    match theme {
        Theme::Dark => crate::design::Palette::Dark,
        Theme::Light => crate::design::Palette::Light,
    }
}

const CURSOR_HINT_DURATION_MS: u64 = 700;

const CURSOR_HINT_FADE_MS: u64 = 100;

const CURSOR_HINT_COOLDOWN_MS: u64 = 30_000;

const ARROW_TOOL_ICON: &str = "sharpArrowIcon";

const CARD_LIME: u32 = 0x74b816;
const CARD_LIME_DARKER: u32 = 0x66a80f;

/// Monotonic id handed to copied images so repeated clipboard writes are not
/// mistaken for the same asset.
static CLIPBOARD_IMAGE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn position_beside_cursor(
    cursor: (f64, f64),
    element: (f64, f64),
    container: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    fn axis(cursor: f64, element: f64, container: f64, gap: f64) -> f64 {
        let position = if cursor + gap + element > container {
            cursor - gap - element
        } else {
            cursor + gap
        };
        position.clamp(0.0, (container - element).max(0.0))
    }
    (
        axis(cursor.0, element.0, container.0, gap),
        axis(cursor.1, element.1, container.1, gap),
    )
}

fn is_dark_hex(hex: &str) -> bool {
    fn channel(digits: &str, index: usize) -> Option<u32> {
        u32::from_str_radix(digits.get(index * 2..index * 2 + 2)?, 16).ok()
    }
    if hex.is_empty() {
        return true;
    }
    if hex.eq_ignore_ascii_case("transparent") {
        return false;
    }
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    let (r, g, b) = match (channel(digits, 0), channel(digits, 1), channel(digits, 2)) {
        (Some(r), Some(g), Some(b)) => (r, g, b),
        _ => return true,
    };
    r * 299 + g * 587 + b * 114 < 160_000
}

fn convert_target_icon(target: ConvertTarget) -> &'static str {
    match target {
        ConvertTarget::Rectangle => "RectangleIcon",
        ConvertTarget::Diamond => "DiamondIcon",
        ConvertTarget::Ellipse => "EllipseIcon",
        ConvertTarget::Line => "LineIcon",
        ConvertTarget::SharpArrow => "sharpArrowIcon",
        ConvertTarget::CurvedArrow => "roundArrowIcon",
        ConvertTarget::ElbowArrow => "elbowArrowIcon",
    }
}

fn convert_target_slug(target: ConvertTarget) -> &'static str {
    match target {
        ConvertTarget::Rectangle => "rectangle",
        ConvertTarget::Diamond => "diamond",
        ConvertTarget::Ellipse => "ellipse",
        ConvertTarget::Line => "line",
        ConvertTarget::SharpArrow => "sharpArrow",
        ConvertTarget::CurvedArrow => "curvedArrow",
        ConvertTarget::ElbowArrow => "elbowArrow",
    }
}

pub fn split_hint(s: &str) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let (head, tail) = rest.split_at(start);
        if !head.is_empty() {
            parts.push((head.to_owned(), false));
        }
        match tail[2..].find("}}") {
            Some(end) => {
                parts.push((tail[2..2 + end].to_owned(), true));
                rest = &tail[2 + end + 2..];
            }
            None => {
                parts.push((tail.to_owned(), false));
                return parts;
            }
        }
    }
    if !rest.is_empty() {
        parts.push((rest.to_owned(), false));
    }
    parts
}

fn plain_text(s: &str) -> AnyElement {
    div()
        .child(SharedString::from(s.to_owned()))
        .into_any_element()
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        self.viewport = (
            window.viewport_size().width.to_f64(),
            window.viewport_size().height.to_f64(),
        );
        self.sync_image_sources();
        if self.pending_image_pick {
            self.pending_image_pick = false;
            self.open_image_picker(cx);
        }
        if self.pending_scene_open {
            self.pending_scene_open = false;
            self.open_scene_picker(cx);
        }
        if self.pending_save_dialog {
            self.pending_save_dialog = false;
            self.open_save_dialog();
        }
        if self.pending_scene_save {
            self.pending_scene_save = false;
            self.save_scene_picker(cx);
        }
        if self.pending_image_dialog {
            self.pending_image_dialog = false;
            self.open_image_dialog();
        }
        if self.pending_copy_png {
            self.pending_copy_png = false;
            self.export_dialog_copy_png(cx);
        }
        if self.pending_copy_svg {
            self.pending_copy_svg = false;
            self.export_dialog_copy_svg(cx);
        }
        let active_tool = self.document.tool;
        let zoom_pct = (self.document.zoom * 100.0).round() as i32;
        let t = |k: &str| self.i18n.t(k);

        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if !focus_handle.is_focused(window) {
            focus_handle.focus(window, cx);
        }

        let mut toolbar = chrome::toolbar_island(tokens)
            .occlude()
            .child(
                chrome::tool_button(
                    "tool-lock",
                    if self.document.tool_locked {
                        "LockedIcon"
                    } else {
                        "UnlockedIcon"
                    },
                    None,
                    self.document.tool_locked,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.tool_locked = !this.document.tool_locked;
                })),
            )
            .child(chrome::toolbar_divider(tokens, true));
        for &(tool, icon_name, shortcut) in TOOL_ROW {
            toolbar = toolbar.child(
                chrome::tool_button(
                    format!("tool-{icon_name}"),
                    icon_name,
                    shortcut,
                    active_tool == tool,
                    tokens,
                )
                .on_click(Self::act(cx, move |this, _, _, _| this.set_tool(tool))),
            );
        }
        let (more_icon, more_selected) = extra_tools_trigger(active_tool);
        let mut toolbar = toolbar.child(chrome::toolbar_divider(tokens, false)).child(
            chrome::tool_button(
                "more-tools",
                more_icon,
                None,
                more_selected || self.show_more_tools,
                tokens,
            )
            .on_click(Self::act(cx, |this, _, _, _| {
                this.set_more_tools_open(!this.show_more_tools);
            })),
        );

        if self.show_more_tools {
            let generate_rows = &MORE_TOOLS[7..];
            let label_of = |mt: &MoreTool| t(mt.key);
            toolbar = toolbar.child(
                chrome::dropdown(tokens)
                    .gap(px(1.0))
                    .min_w(px(190.0))
                    .top(px(size::BUTTON_LG + size::TOOLBAR_PADDING * 2.0 + 8.0))
                    .right(px(size::TOOLBAR_PADDING))
                    .children(MORE_TOOLS[..7].iter().map(|mt| {
                        Self::more_tool_row(mt, &label_of(mt), tokens, cx).into_any_element()
                    }))
                    .child(
                        div()
                            .mt(px(6.0))
                            .mb(px(6.0))
                            .text_size(px(14.0))
                            .font_bold()
                            .text_color(tokens.surface_text())
                            .child(SharedString::from(t("toolbar.generate"))),
                    )
                    .children(generate_rows.iter().map(|mt| {
                        Self::more_tool_row(mt, &label_of(mt), tokens, cx).into_any_element()
                    })),
            );
        }

        let scene = self.document.scene.clone();
        let images = self.image_sources.clone();
        let zoom = self.document.zoom;
        let scroll_x = self.document.scroll.x;
        let scroll_y = self.document.scroll.y;
        let sel_bounds = selection_bounds(&self.document.scene, &self.document.selected);
        let show_grid = self.document.show_grid;
        let grid_size = self.document.grid_size;
        let marquee_bounds = match (self.marquee_start, self.marquee_current) {
            (Some(start), Some(current)) => Some(Bounds::new(
                start.x.min(current.x),
                start.y.min(current.y),
                start.x.max(current.x),
                start.y.max(current.y),
            )),
            _ => None,
        };
        let laser = self.laser_trail();
        let lasso = self.lasso_path.clone();
        let linear_points = self.document.selected_linear_points().map(|(_, p)| p);
        let pending = self.pending_points.clone();
        let pending_cursor = self.cursor_world;

        let preview = self.drag_preview();
        let ink = self.freedraw_preview().map(|p| p.to_vec());
        let ink_width = self.document.style.stroke_width;
        let ink_color = crate::properties::parse_hex(&self.document.style.stroke_color);
        let selection_color = tokens.accent();

        let view_bg = if self.document.view_background_color == "#ffffff"
            || self.document.view_background_color == "transparent"
        {
            tokens.canvas()
        } else {
            crate::properties::parse_hex(&self.document.view_background_color)
        };
        let canvas_area = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .bg(view_bg)
            .on_mouse_down(
                MouseButton::Left,
                Self::act(cx, |this, e: &MouseDownEvent, _, cx| {
                    this.on_canvas_mouse_down(
                        e.position.x.to_f64() as f32,
                        e.position.y.to_f64() as f32,
                        e.modifiers,
                        e.click_count,
                    );
                    if this.laser_active {
                        this.start_laser_fade(cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                Self::act(cx, |this, _: &MouseUpEvent, _, cx| {
                    if this.pending_bucket_fill.is_some() {
                        this.commit_bucket_fill(cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                Self::act(cx, |this, e: &MouseDownEvent, _, _| {
                    let world = this
                        .screen_point(e.position.x.to_f64() as f32, e.position.y.to_f64() as f32);
                    if let Some(id) = this.document.element_at(world, 5.0 / this.document.zoom)
                        && !this.document.selected.contains(&id)
                    {
                        this.document.select(&id);
                    }
                    this.context_menu =
                        Some(Point::new(e.position.x.to_f64(), e.position.y.to_f64()));
                }),
            )
            .on_scroll(Self::act(cx, |this, e: &ScrollEvent, _, _| {
                let (dx, dy) = match e.delta {
                    ScrollDelta::Pixels(p) => (p.x.to_f64(), p.y.to_f64()),
                    ScrollDelta::Lines(p) => (p.x as f64 * 20.0, p.y as f64 * 20.0),
                };
                if e.modifiers.control || e.modifiers.platform {
                    let old = this.document.zoom;
                    if dy > 0.0 {
                        this.on_zoom_out();
                    } else {
                        this.on_zoom_in();
                    }

                    let factor = this.document.zoom / old;
                    if factor != 1.0 {
                        let cx_pos = e.position.x.to_f64();
                        let cy_pos = e.position.y.to_f64();
                        this.document.scroll.x =
                            cx_pos - (cx_pos - this.document.scroll.x) * factor;
                        this.document.scroll.y =
                            cy_pos - (cy_pos - this.document.scroll.y) * factor;
                    }
                } else {
                    this.document.scroll.x -= dx;
                    this.document.scroll.y -= dy;
                }
            }))
            .child(
                canvas(
                    move |_bounds, _window, _cx| (),
                    move |bounds, _state, window, cx| {
                        if show_grid {
                            crate::canvas::paint_grid(
                                grid_size,
                                zoom,
                                scroll_x,
                                scroll_y,
                                (bounds.size.width.to_f64(), bounds.size.height.to_f64()),
                                window,
                            );
                        }
                        crate::canvas::paint_scene(
                            &scene, zoom, scroll_x, scroll_y, &images, window, cx,
                        );
                        if let Some(el) = &preview {
                            crate::canvas::paint_elements(
                                std::slice::from_ref(el),
                                zoom,
                                scroll_x,
                                scroll_y,
                                &images,
                                window,
                                cx,
                            );
                        }
                        if let Some(pts) = &ink {
                            crate::canvas::paint_ink(
                                pts, ink_width, ink_color, zoom, scroll_x, scroll_y, window,
                            );
                        }
                        if let Some(pts) = &linear_points {
                            crate::canvas::paint_linear_handles(
                                pts, zoom, scroll_x, scroll_y, window,
                            );
                        }
                        if !pending.is_empty() {
                            crate::canvas::paint_guide(
                                &pending,
                                pending_cursor,
                                zoom,
                                scroll_x,
                                scroll_y,
                                window,
                            );
                        }
                        if let Some(b) = &sel_bounds {
                            crate::canvas::paint_selection_overlay(
                                b,
                                selection_color,
                                zoom,
                                scroll_x,
                                scroll_y,
                                window,
                            );
                        }
                        if !laser.is_empty() {
                            crate::canvas::paint_laser(&laser, zoom, scroll_x, scroll_y, window);
                        }
                        if let Some(b) = &marquee_bounds {
                            crate::canvas::paint_marquee(
                                b,
                                selection_color,
                                zoom,
                                scroll_x,
                                scroll_y,
                                window,
                            );
                        }
                        if lasso.len() >= 2 {
                            crate::canvas::paint_lasso(
                                &lasso,
                                selection_color,
                                zoom,
                                scroll_x,
                                scroll_y,
                                window,
                            );
                        }
                    },
                )
                .size_full(),
            );

        let hint_text = self.status.clone().unwrap_or_else(|| t("hint.moveCanvas"));

        let hint_children: Vec<AnyElement> = split_hint(&hint_text)
            .into_iter()
            .map(|(run, is_key)| {
                if is_key {
                    chrome::kbd_chip(run, tokens).into_any_element()
                } else {
                    plain_text(&run)
                }
            })
            .collect();

        let toolbar_bottom =
            size::CONTAINER_PADDING + size::TOOLBAR_PADDING * 2.0 + size::BUTTON_LG;
        let dock_top = toolbar_bottom + size::CONTAINER_PADDING;
        let hint_top = toolbar_bottom + 8.0;
        let hint_bar = div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(hint_top))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(tokens.shortcut_text())
                    .children(hint_children),
            );

        let menu_island = chrome::rail_island(tokens)
            .absolute()
            .left(px(size::CONTAINER_PADDING))
            .top(px(size::CONTAINER_PADDING))
            .occlude()
            .child(
                chrome::tool_button("menu", "HamburgerMenuIcon", None, self.show_menu, tokens)
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.set_menu_open(!this.show_menu);
                    })),
            );

        let top_centre = div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(size::CONTAINER_PADDING))
            .flex()
            .flex_row()
            .justify_center()
            .child(toolbar);

        let right_rail = chrome::rail_island(tokens)
            .absolute()
            .right(px(size::CONTAINER_PADDING))
            .top(px(size::CONTAINER_PADDING))
            .occlude()
            .child(
                chrome::tool_button(
                    "library",
                    "sidebarRightIcon",
                    None,
                    self.document.show_library,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| this.on_toggle_library())),
            )
            .child(
                chrome::tool_button(
                    "theme",
                    if tokens.palette.is_dark() {
                        "SunIcon"
                    } else {
                        "MoonIcon"
                    },
                    None,
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| this.on_toggle_theme())),
            );

        let undo_enabled = self.history.can_undo();
        let redo_enabled = self.history.can_redo();

        let footer = div()
            .absolute()
            .left(px(size::CONTAINER_PADDING))
            .bottom(px(size::CONTAINER_PADDING))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .child(
                chrome::rail_island(tokens)
                    .occlude()
                    .child(
                        chrome::chrome_button("zoom-out", "ZoomOutIcon", true, tokens)
                            .on_click(Self::act(cx, |this, _, _, _| this.on_zoom_out())),
                    )
                    .child(
                        chrome::zoom_readout(tokens)
                            .child(SharedString::from(format!("{zoom_pct}%"))),
                    )
                    .child(
                        chrome::chrome_button("zoom-in", "ZoomInIcon", true, tokens)
                            .on_click(Self::act(cx, |this, _, _, _| this.on_zoom_in())),
                    ),
            )
            .child(
                chrome::rail_island(tokens)
                    .occlude()
                    .child(
                        chrome::chrome_button("undo", "UndoIcon", undo_enabled, tokens)
                            .on_click(Self::act(cx, |this, _, _, _| this.on_undo())),
                    )
                    .child(
                        chrome::chrome_button("redo", "RedoIcon", redo_enabled, tokens)
                            .on_click(Self::act(cx, |this, _, _, _| this.on_redo())),
                    ),
            );

        let panel_limit =
            (window.viewport_size().height.to_f64() as f32 - dock_top - 16.0).max(120.0);

        let properties =
            if self.document.selected.is_empty() && self.document.tool != Tool::BucketFill {
                div().into_any_element()
            } else {
                crate::properties::render_properties_panel(self, cx)
                    .absolute()
                    .left(px(size::CONTAINER_PADDING))
                    .top(px(dock_top))
                    .max_h(px(panel_limit))
                    .occlude()
                    .into_any_element()
            };

        let library_panel = if self.document.show_library {
            crate::library::render_library_panel(self, cx)
                .absolute()
                .right(px(size::CONTAINER_PADDING))
                .top(px(dock_top))
                .max_h(px(panel_limit))
                .occlude()
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let layers_panel = if self.document.show_layers {
            crate::layers::render_layers_panel(self, cx)
                .absolute()
                .right(px(size::CONTAINER_PADDING))
                .bottom(px(size::CONTAINER_PADDING + size::BUTTON_LG + 8.0))
                .max_h(px(panel_limit))
                .occlude()
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let stats_island = if self.document.show_stats {
            self.render_stats_island(tokens).into_any_element()
        } else {
            div().into_any_element()
        };

        let zen = self.document.zen_mode;

        let mut root = div()
            .size_full()
            .overflow_hidden()
            .bg(tokens.canvas())
            .track_focus(&focus_handle)
            .on_key_down(Self::act(cx, |this, e: &KeyDownEvent, window, cx| {
                let key = e.keystroke.key.clone();
                let char = e.keystroke.key_char.clone();
                let modifiers = e.keystroke.modifiers;

                // The filename field of the save and export modals owns the clipboard
                // while either is up; without this the element clipboard below
                // would paste onto the canvas behind them.
                if (this.show_save_dialog || this.show_image_dialog)
                    && (modifiers.control || modifiers.platform)
                {
                    match key.as_str() {
                        "c" => {
                            this.save_name_copy(cx);
                            return;
                        }
                        "x" => {
                            this.save_name_cut(cx);
                            return;
                        }
                        "v" => {
                            this.save_name_paste(cx);
                            return;
                        }
                        _ => {}
                    }
                }

                if this.show_mermaid_dialog && (modifiers.control || modifiers.platform) {
                    match key.as_str() {
                        "c" => {
                            this.mermaid_copy(cx);
                            return;
                        }
                        "x" => {
                            this.mermaid_cut(cx);
                            return;
                        }
                        "v" => {
                            this.mermaid_paste(cx);
                            return;
                        }
                        _ => {}
                    }
                }

                if (modifiers.control || modifiers.platform)
                    && !modifiers.shift
                    && key == "v"
                    && !this.show_mermaid_dialog
                {
                    if this.editing_text.is_none() && this.document.has_clipboard() {
                    } else {
                        if this.editing_text.is_none() && this.paste_image_from_clipboard(cx) {
                            return;
                        }
                        if this.paste_text_from_clipboard(cx) {
                            return;
                        }
                    }
                }
                let size = window.viewport_size();
                let vp = (size.width.to_f64(), size.height.to_f64());

                this.note_tool_shortcut(&key, modifiers, cx);
                this.handle_key(&key, char, modifiers, vp);
            }))
            .on_key_up(Self::act(cx, |this, e: &KeyUpEvent, _, _| {
                if e.keystroke.key == "space" {
                    this.space_pan = false;
                    this.panning = false;
                }
            }))
            .on_mouse_move(Self::act(cx, |this, e: &MouseMoveEvent, _, _| {
                if this.is_interacting() {
                    this.on_canvas_mouse_move(
                        e.position.x.to_f64() as f32,
                        e.position.y.to_f64() as f32,
                        e.modifiers,
                    );
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                Self::act(cx, |this, e: &MouseUpEvent, _, _| {
                    if this.is_interacting() {
                        this.on_canvas_mouse_up(
                            e.position.x.to_f64() as f32,
                            e.position.y.to_f64() as f32,
                            e.modifiers,
                        );
                    }
                }),
            )
            .on_drag_move::<ExternalPaths>(Self::act(
                cx,
                |this, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let paths = e.drag(cx).paths().to_vec();
                    this.on_external_drag(&paths, e.event.modifiers.shift);
                },
            ))
            .on_file_drop_exit(Self::act(cx, |this, _: &FileDropEvent, _, _| {
                this.on_external_drag_exit();
            }))
            .on_drop(Self::act(cx, |this, paths: &ExternalPaths, _, cx| {
                if this.handle_external_drop(paths) {
                    let message: String = split_hint(&this.i18n.t("fileDrop.replacedToast"))
                        .into_iter()
                        .map(|(run, _)| run)
                        .collect();
                    this.show_toast(message, true, 8000, cx);
                }
            }))
            .child(canvas_area);

        if !zen {
            if let Some(panel) = self.render_canvas_buttons(cx) {
                root = root.child(panel);
            }
            if let Some(popup) = self.render_unlock_popup(cx) {
                root = root.child(popup);
            }
            if let Some(panel) = self.render_shape_switch(cx) {
                root = root.child(panel);
            }
        }

        if self.welcome && !zen && self.document.scene.is_empty() {
            root = root.child(self.render_welcome(cx));
        }

        if !zen {
            root = root
                .child(hint_bar)
                .child(menu_island)
                .child(top_centre)
                .child(right_rail)
                .child(footer)
                .child(properties)
                .child(library_panel)
                .child(layers_panel);

            let popup_left =
                size::CONTAINER_PADDING + size::PROPERTIES_PANEL_WIDTH + size::MENU_PANEL_GAP;

            let popup_left = if self.color_popup() == Some(ColorTarget::CanvasBackground) {
                size::CONTAINER_PADDING + size::MENU_WIDTH + size::MENU_PANEL_GAP
            } else {
                popup_left
            };
            let popup_top = if self.color_popup() == Some(ColorTarget::CanvasBackground) {
                size::CONTAINER_PADDING + size::BUTTON_LG + 8.0
            } else {
                dock_top
            };
            if self.color_popup().is_some() {
                root = root.child(
                    crate::properties::render_color_picker(self, cx)
                        .absolute()
                        .left(px(popup_left))
                        .top(px(popup_top))
                        .max_h(px(panel_limit))
                        .occlude(),
                );
            }
            if self.is_font_popup_open() {
                root = root.child(
                    crate::properties::render_font_picker(self, cx)
                        .absolute()
                        .left(px(popup_left))
                        .top(px(dock_top))
                        .max_h(px(panel_limit))
                        .occlude(),
                );
            }
            if self.is_link_dialog_open()
                && let Some(bounds) =
                    selection_bounds(&self.document.scene, &self.document.selected)
            {
                let zoom = self.document.zoom;
                let center_x = self.document.scroll.x
                    + (bounds.min_x + (bounds.max_x - bounds.min_x) / 2.0) * zoom;
                let viewport = window.viewport_size();
                let min_left = size::CONTAINER_PADDING as f64;
                let max_left = (viewport.width.to_f64()
                    - size::link_dialog::WIDTH as f64
                    - size::CONTAINER_PADDING as f64)
                    .max(min_left);
                let left =
                    (center_x - size::link_dialog::WIDTH as f64 / 2.0).clamp(min_left, max_left);
                let top = (self.document.scroll.y + bounds.min_y * zoom
                    - size::link_dialog::TOP_OFFSET as f64)
                    .max(min_left);
                root = root.child(
                    crate::properties::render_link_dialog(self, cx)
                        .absolute()
                        .left(px(left as f32))
                        .top(px(top as f32)),
                );
            }
        }
        root = root.child(stats_island);

        if self.show_menu {
            root = root.child(self.render_file_menu(cx));
            if self.show_prefs {
                root = root.child(self.render_prefs_panel(cx));
            }

            if self.show_lang {
                root = root.child(self.render_language_panel(cx));
            }
        }
        if self.context_menu.is_some() {
            root = root.child(self.render_context_menu(cx));
        }

        if self.show_find {
            root = root.child(self.render_find(cx));
        }
        if self.show_help {
            root = root.child(self.render_help(cx));
        }
        if self.show_clear_dialog {
            root = root.child(self.render_clear_canvas_dialog(cx));
        }
        if self.show_save_dialog {
            root = root.child(self.render_save_dialog(cx));
        }
        if self.show_image_dialog {
            root = root.child(self.render_image_dialog(cx));
        }
        if self.show_mermaid_dialog {
            root = root.child(self.render_mermaid_dialog(cx));
        }
        if self.show_palette {
            root = root.child(self.render_palette(cx));
        }

        if let Some(overlay) = self.render_eye_dropper(cx) {
            root = root.child(overlay);
        }
        if let Some(hint) = self.render_cursor_hint(tokens) {
            root = root.child(hint);
        }
        if self.file_drag.is_some() {
            root = root.child(self.render_file_drop_overlay(tokens));
        }

        if self.is_interacting() {
            root = root.child(self.render_gesture_capture(cx));
        }

        if let Some(toast) = self.render_toast(tokens, cx) {
            root = root.child(toast);
        }

        root
    }
}

impl Editor {
    fn more_tool_row(
        mt: &MoreTool,
        label: &str,
        tokens: crate::design::Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut row = chrome::menu_row_with(
            format!("more-{}", mt.icon),
            Some(mt.icon),
            label,
            mt.shortcut,
            false,
            tokens,
        );
        if mt.ai {
            let badge = div()
                .flex_shrink_0()
                .px(px(4.0))
                .py(px(2.0))
                .mr(px(2.0))
                .rounded(px(6.0))
                .bg(tokens.accent())
                .font_family("monospace")
                .text_size(px(9.0))
                .text_color(tokens.surface_lowest())
                .child("AI");
            row = row.child(if tokens.palette.is_dark() {
                badge
            } else {
                badge.border_1().border_color(rgb(0xffffff))
            });
        }
        match &mt.action {
            MoreToolAction::Tool(tool) => {
                let tool = *tool;
                row.on_click(Self::act(cx, move |this, _, _, _| {
                    this.set_tool(tool);
                    this.show_more_tools = false;
                }))
            }
            MoreToolAction::Mermaid => row.on_click(Self::act(cx, move |this, _, _, _| {
                this.open_mermaid_dialog();
                this.show_more_tools = false;
            })),
            MoreToolAction::Stub => row.on_click(Self::act(cx, move |this, _, _, _| {
                this.status = Some(this.i18n.t("status.unavailable"));
                this.show_more_tools = false;
            })),
        }
    }

    fn prefs_segment(
        id_base: &'static str,
        label: &str,
        options: [&'static str; 2],
        active: usize,
        tokens: crate::design::Tokens,
        cx: &mut Context<Self>,
        on_pick: impl Fn(&mut Self, usize) + 'static + Copy,
    ) -> impl IntoElement {
        let mut row = div()
            .id(SharedString::from(format!("{id_base}-row")))
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .w_full()
            .min_h(px(size::MENU_ROW))
            .px(px(8.0))
            .text_size(px(14.0))
            .child(div().flex_1().child(SharedString::from(label.to_owned())));
        let mut group = div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(8.0))
            .bg(tokens.hover());
        for (i, opt) in options.iter().enumerate() {
            let is_active = i == active;
            let mut pill = div()
                .id(SharedString::from(format!("{id_base}-opt{i}")))
                .test_support()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .child(SharedString::from((*opt).to_owned()));
            pill = if is_active {
                pill.bg(tokens.primary_darkest()).text_color(rgb(0xffffff))
            } else {
                pill.text_color(tokens.surface_text())
                    .hover(move |s| s.bg(tokens.separator()))
            };
            group = group.child(pill.on_click(Self::act(cx, move |this, _, _, _| {
                on_pick(this, i);
            })));
        }
        row = row.child(group);
        row
    }

    fn theme_choice(
        id: &'static str,
        icon_name: &'static str,
        active: bool,
        tokens: crate::design::Tokens,
        cx: &mut Context<Self>,
        on_pick: impl Fn(&mut Self) + 'static + Copy,
    ) -> impl IntoElement {
        let glyph = if active {
            rgb(0xffffff)
        } else {
            tokens.surface_text()
        };
        let mut pill = div()
            .id(SharedString::from(id))
            .test_support()
            .size(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(glyph);
        pill = if active {
            pill.bg(tokens.primary_darkest())
        } else {
            pill.hover(move |s| s.bg(tokens.separator()))
        };
        pill.child(crate::icons::icon(icon_name, 16.0, glyph))
            .on_click(Self::act(cx, move |this, _, _, _| on_pick(this)))
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> Div {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        let fg = tokens.shortcut_text();
        let brand = if tokens.palette.is_dark() {
            crate::properties::parse_hex("#a7b6ff")
        } else {
            crate::properties::parse_hex("#6965db")
        };

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(24.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(crate::icons::icon("logoMark", 40.0, brand))
                    .child(
                        svg()
                            .path(crate::icons::icon_path("logoWord"))
                            .flex_shrink_0()
                            .w(px(186.0))
                            .h(px(23.0))
                            .text_color(brand),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.0))
                    .text_size(px(14.0))
                    .text_color(fg)
                    .child(SharedString::from(t("welcome.center1")))
                    .child(SharedString::from(t("welcome.center2")))
                    .child(SharedString::from(t("welcome.center3"))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .id("wl-open")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(12.0))
                            .w(px(300.0))
                            .min_h(px(40.0))
                            .px(px(8.0))
                            .rounded(px(8.0))
                            .text_size(px(14.0))
                            .text_color(fg)
                            .hover(move |s| s.bg(tokens.hover()))
                            .child(crate::icons::icon("LoadIcon", 18.0, fg))
                            .child(div().flex_1().child(SharedString::from(t("menu.open"))))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .child(SharedString::from("Ctrl+O")),
                            )
                            .on_click(Self::act(cx, |this, _, _, _| {
                                this.pending_scene_open = true;
                            })),
                    )
                    .child(
                        div()
                            .id("wl-help")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(12.0))
                            .w(px(300.0))
                            .min_h(px(40.0))
                            .px(px(8.0))
                            .rounded(px(8.0))
                            .text_size(px(14.0))
                            .text_color(fg)
                            .hover(move |s| s.bg(tokens.hover()))
                            .child(crate::icons::icon("HelpIcon", 18.0, fg))
                            .child(div().flex_1().child(SharedString::from(t("menu.help"))))
                            .child(div().text_size(px(12.0)).child(SharedString::from("?")))
                            .on_click(Self::act(cx, |this, _, _, _| {
                                this.status = Some(this.i18n.t("menu.helpHint"));
                            })),
                    )
                    .child(
                        div()
                            .id("wl-collab")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(12.0))
                            .w(px(300.0))
                            .min_h(px(40.0))
                            .px(px(8.0))
                            .rounded(px(8.0))
                            .text_size(px(14.0))
                            .text_color(fg)
                            .hover(move |s| s.bg(tokens.hover()))
                            .child(crate::icons::icon("collabIcon", 18.0, fg))
                            .child(div().flex_1().child(SharedString::from(t("menu.collab"))))
                            .on_click(Self::act(cx, |this, _, _, _| {
                                this.status = Some(this.i18n.t("status.unavailable"));
                            })),
                    )
                    .child(
                        div()
                            .id("wl-signup")
                            .test_support()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(12.0))
                            .w(px(300.0))
                            .min_h(px(40.0))
                            .px(px(8.0))
                            .rounded(px(8.0))
                            .text_size(px(14.0))
                            .text_color(fg)
                            .hover(move |s| s.bg(tokens.hover()))
                            .child(crate::icons::icon("signUpIcon", 18.0, fg))
                            .child(div().flex_1().child(SharedString::from(t("menu.signUp"))))
                            .on_click(Self::act(cx, |_, _, _, _| {
                                let _ = std::process::Command::new("xdg-open")
                                    .arg("https://excalidraw.com")
                                    .spawn();
                            })),
                    ),
            )
    }

    fn render_stats_island(&self, tokens: crate::design::Tokens) -> Div {
        let t = |k: &str| self.i18n.t(k);
        let fg = tokens.surface_text();
        let muted = tokens.shortcut_text();

        let mut rows = vec![(
            t("stats.shapes"),
            self.document.scene.non_deleted().count().to_string(),
        )];
        if let Some(el) = self
            .document
            .scene
            .non_deleted()
            .find(|e| self.document.selected.contains(e.id()))
        {
            let b = el.base();
            rows.push((t("stats.selectedType"), format!("{:?}", el.kind())));
            rows.push(("x".to_string(), format!("{:.0}", b.x)));
            rows.push(("y".to_string(), format!("{:.0}", b.y)));
            rows.push(("w".to_string(), format!("{:.0}", b.width)));
            rows.push(("h".to_string(), format!("{:.0}", b.height)));
            rows.push(("∠".to_string(), format!("{:.1}°", b.angle.to_degrees())));
        }

        let mut island = chrome::rail_island(tokens)
            .absolute()
            .right(px(size::CONTAINER_PADDING))
            .top(px(size::CONTAINER_PADDING + size::BUTTON_LG + 16.0))
            .w(px(200.0))
            .p(px(12.0))
            .occlude()
            .child(
                div()
                    .font_bold()
                    .text_size(px(13.0))
                    .text_color(fg)
                    .pb(px(6.0))
                    .child(SharedString::from(t("stats.title"))),
            );
        for (label, value) in rows {
            island = island.child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .py(px(1.0))
                    .text_size(px(12.0))
                    .child(div().text_color(muted).child(SharedString::from(label)))
                    .child(div().text_color(fg).child(SharedString::from(value))),
            );
        }
        island
    }

    fn render_file_menu(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);

        let top = size::CONTAINER_PADDING + size::BUTTON_LG + 8.0;

        let rule = || {
            div()
                .h(px(1.0))
                .my(px(4.0))
                .mx(px(4.0))
                .bg(tokens.separator())
        };
        let promo = tokens.primary_darkest();

        chrome::dropdown(tokens)
            .id("file-menu")
            .test_support()
            .w(px(size::MENU_WIDTH))
            .left(px(size::CONTAINER_PADDING))
            .top(px(top))
            .max_h(px(self.menu_max_height()))
            .overflow_y_scroll()
            .child(
                chrome::menu_row_with(
                    "fm-open",
                    Some("LoadIcon"),
                    &t("menu.open"),
                    Some("Ctrl+O"),
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.pending_scene_open = true;
                    this.show_menu = false;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "fm-save",
                    Some("ExportIcon"),
                    &t("menu.save"),
                    None,
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.pending_save_dialog = true;
                    this.show_menu = false;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "fm-png",
                    Some("ImageIcon"),
                    &t("menu.exportImage"),
                    Some("Ctrl+Shift+E"),
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.pending_image_dialog = true;
                    this.show_menu = false;
                })),
            )
            .child(
                chrome::menu_row_colored(
                    "fm-palette",
                    Some("commandPaletteIcon"),
                    &t("menu.commandPalette"),
                    Some("Ctrl+/"),
                    false,
                    tokens,
                    promo,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.open_palette();
                })),
            )
            .child(
                chrome::menu_row_with(
                    "fm-find",
                    Some("searchIcon"),
                    &t("menu.findOnCanvas"),
                    Some("Ctrl+F"),
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.open_find();
                })),
            )
            .child(
                chrome::menu_row_with(
                    "fm-help",
                    Some("HelpIcon"),
                    &t("menu.help"),
                    Some("?"),
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.open_help();
                })),
            )
            .child(
                chrome::menu_row_with(
                    "fm-reset",
                    Some("TrashIcon"),
                    &t("menu.reset"),
                    None,
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.show_clear_dialog = true;
                    this.show_menu = false;
                })),
            )
            .child(rule())
            .child(
                chrome::menu_row_with(
                    "fm-prefs",
                    Some("slidersIcon"),
                    &t("menu.preferences"),
                    Some("›"),
                    false,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.show_prefs = !this.show_prefs;
                })),
            )
            .child(
                div()
                    .id("fm-theme-row")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .w_full()
                    .min_h(px(size::MENU_ROW))
                    .px(px(8.0))
                    .text_size(px(14.0))
                    .child(div().flex_1().child(SharedString::from(t("menu.theme"))))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_shrink_0()
                            .gap(px(2.0))
                            .p(px(2.0))
                            .rounded(px(8.0))
                            .bg(tokens.hover())
                            .child(Self::theme_choice(
                                "theme-light",
                                "themeLightIcon",
                                self.document.theme == Theme::Light,
                                tokens,
                                cx,
                                |this| this.document.set_theme(Theme::Light),
                            ))
                            .child(Self::theme_choice(
                                "theme-dark",
                                "themeDarkIcon",
                                self.document.theme == Theme::Dark,
                                tokens,
                                cx,
                                |this| this.document.set_theme(Theme::Dark),
                            ))
                            .child(Self::theme_choice(
                                "theme-system",
                                "themeSystemIcon",
                                false,
                                tokens,
                                cx,
                                |this| this.on_toggle_theme(),
                            )),
                    ),
            )
            .child(
                Self::language_select(SharedString::from(self.i18n.language().label()), tokens)
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.show_lang = !this.show_lang;
                        this.show_prefs = false;
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .px(px(12.0))
                    .py(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(tokens.shortcut_text())
                            .child(SharedString::from(t("menu.canvasBackground"))),
                    )
                    .child({
                        let mut picks = div().flex().flex_row().items_center().gap(px(5.0));
                        for hex in ["#ffffff", "#f8f9fa", "#f5faff", "#fffce8", "#fdf8f6"] {
                            let active = self
                                .document
                                .view_background_color
                                .eq_ignore_ascii_case(hex);
                            let mut swatch = div()
                                .id(SharedString::from(format!(
                                    "viewbg-{}",
                                    hex.trim_start_matches('#')
                                )))
                                .test_support()
                                .size(px(22.0))
                                .rounded(px(4.0))
                                .bg(crate::properties::parse_hex(hex));
                            swatch = if active {
                                swatch.border_1().border_color(tokens.primary_darkest())
                            } else if crate::properties::needs_outline(hex) {
                                swatch.border_1().border_color(tokens.swatch_outline())
                            } else {
                                swatch
                            };
                            picks = picks.child(swatch.on_click(Self::act(
                                cx,
                                move |this, _, _, _| {
                                    this.document.view_background_color = hex.to_string();
                                },
                            )));
                        }
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .child(picks)
                            .child(crate::properties::button_separator(tokens))
                            .child(crate::properties::color_trigger(
                                "viewbg-custom",
                                ColorTarget::CanvasBackground,
                                &self.document.view_background_color,
                                self.color_popup() == Some(ColorTarget::CanvasBackground),
                                tokens,
                                cx,
                            ))
                    }),
            )
            .into_any_element()
    }

    fn language_select(label: SharedString, tokens: Tokens) -> chrome::Button {
        let fg = tokens.surface_text();
        let caret = size::language_select::CARET;
        div()
            .id("fm-lang")
            .test_support()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(size::language_select::HEIGHT))
            .pl(px(size::language_select::PAD_START))
            .pr(px(size::language_select::PAD_END))
            .rounded(px(size::language_select::RADIUS))
            .border_1()
            .border_color(tokens.island_border())
            .bg(tokens.island())
            .text_size(px(size::language_select::FONT))
            .text_color(fg)
            .child(div().flex_1().child(label))
            .child(
                div()
                    .absolute()
                    .right(px(size::language_select::CARET_RIGHT))
                    .top(px(size::language_select::CARET_TOP))
                    .child(crate::icons::icon("dropdownIcon", caret, fg)),
            )
            .hover(move |s| s.bg(tokens.hover()))
    }

    fn render_language_panel(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let current = self.i18n.language();
        let top = size::CONTAINER_PADDING + size::BUTTON_LG + 8.0;
        let mut panel = chrome::dropdown(tokens)
            .id("lang-list")
            .test_support()
            .left(px(size::CONTAINER_PADDING
                + size::MENU_WIDTH
                + size::MENU_PANEL_GAP))
            .top(px(top))
            .max_h(px(self.menu_max_height()))
            .overflow_y_scroll();
        for language in Language::all() {
            let selected = language == current;
            let fg = if selected {
                tokens.selected_fg()
            } else {
                tokens.surface_text()
            };
            let row = div()
                .id(SharedString::from(format!("lang-{}", language.code())))
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .min_h(px(size::MENU_ROW))
                .px(px(8.0))
                .rounded(px(size::RADIUS_MD))
                .text_size(px(14.0))
                .text_color(fg)
                .child(SharedString::from(language.label()));
            let row = if selected {
                row.bg(tokens.selected_bg())
            } else {
                row.hover(move |s| s.bg(tokens.hover()))
            };
            panel = panel.child(row.on_click(Self::act(cx, move |this, _, _, _| {
                this.set_language(language);
                this.show_lang = false;
            })));
        }
        panel.into_any_element()
    }

    fn render_prefs_panel(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        let top = size::CONTAINER_PADDING + size::BUTTON_LG + 8.0;
        chrome::dropdown(tokens)
            .min_w(px(250.0))
            .left(px(size::CONTAINER_PADDING
                + size::MENU_WIDTH
                + size::MENU_PANEL_GAP))
            .top(px(top))
            .child(Self::prefs_segment(
                "prefs-select",
                &t("prefs.selectOn"),
                ["Wrap", "Overlap"],
                if self.document.select_wrap { 0 } else { 1 },
                tokens,
                cx,
                |this, i| this.document.select_wrap = i == 0,
            ))
            .child(Self::prefs_segment(
                "prefs-input",
                &t("prefs.input"),
                ["Trackpad", "Mouse"],
                if self.document.input_trackpad { 0 } else { 1 },
                tokens,
                cx,
                |this, i| this.document.input_trackpad = i == 0,
            ))
            .child(
                chrome::menu_row_with(
                    "prefs-lock",
                    None,
                    &t("menu.toolLock"),
                    Some("Q"),
                    self.document.tool_locked,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.tool_locked = !this.document.tool_locked;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-snap-objects",
                    None,
                    &t("menu.snapToObjects"),
                    Some("Alt+S"),
                    self.document.snap_to_objects,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.snap_to_objects = !this.document.snap_to_objects;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-grid",
                    None,
                    &t("menu.grid"),
                    Some("Ctrl+'"),
                    self.document.show_grid,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.on_toggle_grid();
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-zen",
                    None,
                    &t("menu.zenMode"),
                    Some("Alt+Z"),
                    self.document.zen_mode,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.zen_mode = !this.document.zen_mode;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-view",
                    None,
                    &t("menu.viewMode"),
                    Some("Alt+R"),
                    self.document.view_mode,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.view_mode = !this.document.view_mode;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-stats",
                    None,
                    &t("menu.canvasStats"),
                    Some("Alt+/"),
                    self.document.show_stats,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.show_stats = !this.document.show_stats;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-arrow-binding",
                    None,
                    &t("menu.arrowBinding"),
                    None,
                    self.document.arrow_binding,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.arrow_binding = !this.document.arrow_binding;
                })),
            )
            .child(
                chrome::menu_row_with(
                    "prefs-snap-midpoints",
                    None,
                    &t("menu.snapToMidpoints"),
                    None,
                    self.document.snap_to_midpoints,
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.document.snap_to_midpoints = !this.document.snap_to_midpoints;
                })),
            )
            .into_any_element()
    }

    fn render_gesture_capture(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        div()
            .id("gesture-capture")
            .test_support()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .occlude()
            .on_mouse_move(Self::act(cx, |this, e: &MouseMoveEvent, _, _| {
                this.on_canvas_mouse_move(
                    e.position.x.to_f64() as f32,
                    e.position.y.to_f64() as f32,
                    e.modifiers,
                );
            }))
            .on_mouse_up(
                MouseButton::Left,
                Self::act(cx, |this, e: &MouseUpEvent, _, cx| {
                    this.on_canvas_mouse_up(
                        e.position.x.to_f64() as f32,
                        e.position.y.to_f64() as f32,
                        e.modifiers,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_shape_switch(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        if self.is_interacting() || self.context_menu.is_some() || self.show_menu {
            return None;
        }
        let element = self.document.lone_selected_element()?;
        let targets = element.convertible_targets();
        if targets.is_empty() {
            return None;
        }
        let current = element.convert_target();
        let (left, top) = self.shape_switch_anchor()?;
        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let mut panel = div()
            .id("shape-switch")
            .test_support()
            .absolute()
            .occlude()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(size::SHAPE_SWITCH_GAP))
            .p(px(size::SHAPE_SWITCH_PADDING))
            .rounded(px(size::RADIUS_LG))
            .bg(tokens.island())
            .shadow(crate::design::island_shadow())
            .left(px(left))
            .top(px(top));
        for target in targets {
            panel = panel.child(Self::shape_switch_button(
                *target,
                current == Some(*target),
                tokens,
                cx,
            ));
        }
        Some(panel.into_any_element())
    }

    fn shape_switch_anchor(&self) -> Option<(f32, f32)> {
        let bounds = selection_bounds(&self.document.scene, &self.document.selected)?;
        let zoom = self.document.zoom;
        let x = self.document.scroll.x + bounds.min_x * zoom - size::SHAPE_SWITCH_OFFSET_X as f64;
        let y = self.document.scroll.y
            + bounds.max_y * zoom
            + size::SHAPE_SWITCH_OFFSET_Y as f64 * zoom;
        Some((x as f32, y as f32))
    }

    fn shape_switch_button(
        target: ConvertTarget,
        checked: bool,
        tokens: Tokens,
        cx: &Context<Self>,
    ) -> chrome::Button {
        let fg = if checked {
            tokens.selected_fg()
        } else {
            tokens.surface_text()
        };
        let base = div()
            .id(SharedString::from(format!(
                "shape-{}",
                convert_target_slug(target)
            )))
            .test_support()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(size::BUTTON))
            .rounded(px(size::RADIUS_MD))
            .text_color(fg)
            .child(crate::icons::icon(
                convert_target_icon(target),
                size::ICON,
                fg,
            ));
        let base = if checked {
            base.bg(tokens.selected_bg())
        } else {
            base.hover(move |s| s.bg(tokens.hover()))
        };
        base.on_click(Self::act(cx, move |this, _, _, _| {
            this.on_convert_target(target);
        }))
    }

    fn render_unlock_popup(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        if self.is_interacting()
            || self.context_menu.is_some()
            || self.show_menu
            || self.document.view_mode
        {
            return None;
        }
        let id = self.active_locked_id.clone()?;
        let elements = self.document.locked_target(&id);
        let bounds = crate::core::bounds::common_bounds(&elements)?;
        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let zoom = self.document.zoom;
        let left = self.document.scroll.x + bounds.min_x * zoom;

        let bottom = self.viewport.1 + size::UNLOCK_OFFSET_Y as f64
            - (self.document.scroll.y + bounds.min_y * zoom);
        Some(
            div()
                .id("unlock-popup")
                .test_support()
                .absolute()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(size::UNLOCK_GAP))
                .p(px(size::UNLOCK_PADDING))
                .rounded(px(size::RADIUS_LG))
                .bg(tokens.island())
                .shadow(crate::design::island_shadow())
                .text_color(tokens.gray_60())
                .left(px(left as f32))
                .bottom(px(bottom as f32))
                .child(crate::icons::icon(
                    "LockedIconFilled",
                    size::UNLOCK_ICON,
                    tokens.gray_60(),
                ))
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.on_unlock_locked_target();
                }))
                .into_any_element(),
        )
    }

    fn render_canvas_buttons(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        if self.is_interacting()
            || self.context_menu.is_some()
            || self.show_menu
            || self.document.view_mode
        {
            return None;
        }
        let element = self.document.lone_selected_element()?;
        let buttons: Vec<(&str, CanvasButton)> = match element {
            Element::MagicFrame(_) => vec![("MagicIcon", CanvasButton::ConvertToCode)],
            Element::Iframe(iframe) if iframe_generation_finished(&iframe.base) => vec![
                ("copyIcon", CanvasButton::CopySource),
                ("fullscreenIcon", CanvasButton::Fullscreen),
            ],
            _ => return None,
        };
        let bounds = selection_bounds(&self.document.scene, &self.document.selected)?;
        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let zoom = self.document.zoom;
        let left =
            self.document.scroll.x + bounds.max_x * zoom + size::CANVAS_BUTTONS_OFFSET_X as f64;
        let top = self.document.scroll.y + bounds.min_y * zoom;
        let mut island = div()
            .id("canvas-buttons")
            .test_support()
            .absolute()
            .occlude()
            .flex()
            .flex_col()
            .gap(px(size::CANVAS_BUTTONS_GAP))
            .p(px(size::CANVAS_BUTTONS_PADDING))
            .rounded(px(size::RADIUS_LG))
            .bg(tokens.island())
            .shadow(crate::design::canvas_buttons_shadow())
            .left(px(left as f32))
            .top(px(top as f32));
        for (icon_name, action) in buttons {
            island = island.child(Self::canvas_button(icon_name, action, tokens, cx));
        }
        Some(island.into_any_element())
    }

    fn canvas_button(
        icon_name: &str,
        action: CanvasButton,
        tokens: Tokens,
        cx: &mut Context<Self>,
    ) -> chrome::Button {
        let fg = tokens.surface_text();
        div()
            .id(SharedString::from(format!("canvas-{icon_name}")))
            .test_support()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .size(px(size::BUTTON))
            .rounded(px(size::RADIUS_MD))
            .text_color(fg)
            .child(crate::icons::icon(icon_name, size::ICON, fg))
            .hover(move |s| s.bg(tokens.hover()))
            .on_click(Self::act(cx, move |this, _, _, cx| {
                this.on_canvas_button(action, cx);
            }))
    }

    fn on_canvas_button(&mut self, action: CanvasButton, cx: &mut Context<Self>) {
        match action {
            CanvasButton::CopySource => {
                let source = self
                    .document
                    .lone_selected_element()
                    .and_then(|element| element.base().link.clone())
                    .unwrap_or_default();
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(source));
                self.status = Some(self.i18n.t("labels.copySource"));
            }
            CanvasButton::ConvertToCode | CanvasButton::Fullscreen => {
                self.status = Some(self.i18n.t("status.unavailable"));
            }
        }
    }

    fn next_overlay_nonce(&mut self) -> u64 {
        self.overlay_nonce += 1;
        self.overlay_nonce
    }

    fn arm_overlay_hide<F>(&self, nonce: u64, after_ms: u64, f: F, cx: &mut Context<Self>)
    where
        F: Fn(&mut Self, u64) + 'static,
    {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(after_ms))
                .await;
            let _ = this.update(cx, |editor, cx| {
                f(editor, nonce);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn cursor_hint_icon(&self) -> Option<&'static str> {
        self.cursor_hint.as_ref().map(|hint| hint.icon)
    }

    pub fn toast_message(&self) -> Option<&str> {
        self.toast.as_ref().map(|toast| toast.message.as_str())
    }

    pub fn file_drag_kind(&self) -> Option<&'static str> {
        self.file_drag.map(|drag| match drag.kind {
            DropKind::Scene => "scene",
            DropKind::Library => "library",
            DropKind::Unknown => "unknown",
        })
    }

    pub fn is_eye_dropper_active(&self) -> bool {
        self.eye_dropper.is_some()
    }

    pub(crate) fn eye_dropper_target(&self) -> Option<ColorTarget> {
        self.eye_dropper.as_ref().map(|state| state.target)
    }

    pub fn eye_dropper_color(&self) -> Option<&str> {
        self.eye_dropper.as_ref().map(|state| state.color.as_str())
    }

    fn is_scene_file(path: &std::path::Path) -> bool {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        name.ends_with(".excalidraw") || name.contains(".excalidraw.")
    }

    fn drop_kind(paths: &[std::path::PathBuf]) -> DropKind {
        let mut kind = DropKind::Unknown;
        for path in paths {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if name.ends_with(".excalidrawlib") {
                return DropKind::Library;
            }
            if Self::is_scene_file(path) {
                kind = DropKind::Scene;
            }
        }
        kind
    }

    fn on_external_drag(&mut self, paths: &[std::path::PathBuf], shift: bool) {
        self.file_drag = Some(FileDragState {
            kind: Self::drop_kind(paths),
            shift,
        });
    }

    fn on_external_drag_exit(&mut self) {
        self.file_drag = None;
    }

    fn show_cursor_hint(&mut self, icon: &'static str, cx: &mut Context<Self>) {
        if self.cursor_screen == (0.0, 0.0) {
            return;
        }
        let nonce = self.next_overlay_nonce();
        self.cursor_hint = Some(CursorHintState { icon, nonce });
        self.arm_overlay_hide(
            nonce,
            CURSOR_HINT_DURATION_MS + CURSOR_HINT_FADE_MS,
            |editor, nonce| {
                if editor
                    .cursor_hint
                    .as_ref()
                    .is_some_and(|hint| hint.nonce == nonce)
                {
                    editor.cursor_hint = None;
                }
            },
            cx,
        );
    }

    pub fn note_tool_shortcut(&mut self, key: &str, modifiers: Modifiers, cx: &mut Context<Self>) {
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.shift {
            return;
        }

        if self.show_palette || self.show_find || self.show_help || self.editing_text.is_some() {
            return;
        }
        let (icon, is_digit) = match key {
            "a" => (ARROW_TOOL_ICON, false),
            "5" => (ARROW_TOOL_ICON, true),
            "l" => ("LineIcon", false),
            "6" => ("LineIcon", true),
            _ => return,
        };
        if !is_digit {
            let fresh = self.cursor_hint_shown_at.is_some_and(|shown| {
                shown.elapsed() < std::time::Duration::from_millis(CURSOR_HINT_COOLDOWN_MS)
            });
            if fresh {
                return;
            }
        }
        self.cursor_hint_shown_at = Some(std::time::Instant::now());
        self.show_cursor_hint(icon, cx);
    }

    fn show_toast(
        &mut self,
        message: String,
        closable: bool,
        duration_ms: u64,
        cx: &mut Context<Self>,
    ) {
        let nonce = self.next_overlay_nonce();
        self.toast = Some(ToastState {
            message,
            closable,
            nonce,
        });
        if duration_ms != u64::MAX {
            self.arm_overlay_hide(
                nonce,
                duration_ms,
                |editor, nonce| {
                    if editor
                        .toast
                        .as_ref()
                        .is_some_and(|toast| toast.nonce == nonce)
                    {
                        editor.toast = None;
                    }
                },
                cx,
            );
        }
    }

    pub fn dismiss_toast(&mut self) {
        self.toast = None;
    }

    fn start_eye_dropper(&mut self, target: ColorTarget) {
        let at = self.cursor_screen;
        let color = self.sample_color_at(at);
        self.eye_dropper = Some(EyeDropperState { target, at, color });
    }

    pub(crate) fn toggle_eye_dropper(&mut self, target: ColorTarget) {
        match &self.eye_dropper {
            Some(state) if state.target == target => self.eye_dropper = None,
            _ => self.start_eye_dropper(target),
        }
    }

    fn update_eye_dropper(&mut self, at: (f64, f64)) {
        if self.eye_dropper.is_none() {
            return;
        }
        let color = self.sample_color_at(at);
        if let Some(state) = self.eye_dropper.as_mut() {
            state.at = at;
            state.color = color;
        }
    }

    fn commit_eye_dropper(&mut self) {
        let Some(state) = self.eye_dropper.take() else {
            return;
        };
        match state.target {
            ColorTarget::Stroke => self.document.set_selected_stroke_color(&state.color),
            ColorTarget::Background => self.set_background_color(&state.color),
            ColorTarget::CanvasBackground => self.document.view_background_color = state.color,
        }
    }

    fn cancel_eye_dropper(&mut self) {
        self.eye_dropper = None;
    }

    fn sample_color_at(&self, at: (f64, f64)) -> String {
        let world = self.screen_point(at.0 as f32, at.1 as f32);
        let threshold = 10.0 / self.document.zoom;
        let canvas = self.export_background().to_ascii_lowercase();
        let sampled = self
            .document
            .elements_at_including_locked(world, threshold)
            .last()
            .and_then(|element| {
                let base = element.base();
                let background = base.background_color.to_ascii_lowercase();
                if background != "transparent" && background.starts_with('#') {
                    return Some(background);
                }
                let stroke = base.stroke_color.to_ascii_lowercase();
                (stroke != "transparent" && stroke.starts_with('#')).then_some(stroke)
            });
        sampled.unwrap_or(canvas)
    }

    pub fn handle_external_drop(&mut self, paths: &ExternalPaths) -> bool {
        let list = paths.paths();
        let drag = self.file_drag.take();
        let kind = drag
            .map(|state| state.kind)
            .unwrap_or_else(|| Self::drop_kind(list));
        let keeps = drag
            .map(|state| state.keeps_content())
            .unwrap_or(kind == DropKind::Library);
        if kind == DropKind::Scene
            && !keeps
            && let Some(path) = list.iter().find(|path| Self::is_scene_file(path))
        {
            let path = path.to_string_lossy().to_string();
            if self.load_from(&path).is_ok() {
                return true;
            }
        }
        self.insert_image_paths(list);
        false
    }

    fn render_file_drop_overlay(&self, tokens: Tokens) -> gpui_kit::AnyElement {
        let Some(state) = self.file_drag else {
            return div().into_any_element();
        };
        let t = |k: &str| self.i18n.t(k);
        let keeps = state.keeps_content();
        let title = match state.kind {
            DropKind::Library => t("fileDrop.importLibrary"),
            _ if state.shift => t("fileDrop.add"),
            _ => t("fileDrop.replace"),
        };
        let hint: Vec<AnyElement> = if keeps {
            vec![plain_text(&t("fileDrop.keepHint"))]
        } else {
            split_hint(&t("fileDrop.replaceHint"))
                .into_iter()
                .map(|(run, is_key)| {
                    if is_key {
                        div()
                            .flex_shrink_0()
                            .mx(px(2.0))
                            .px(px(size::file_drop::KBD_PAD_X))
                            .py(px(size::file_drop::KBD_PAD_Y))
                            .rounded(px(size::file_drop::KBD_RADIUS))
                            .border_1()
                            .border_color(tokens.surface_high())
                            .bg(tokens.surface_low())
                            .child(SharedString::from(run))
                            .into_any_element()
                    } else {
                        plain_text(&run)
                    }
                })
                .collect()
        };
        let mut card = div()
            .w(px(size::file_drop::CARD_WIDTH))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(size::file_drop::CARD_PAD_TOP))
            .px(px(size::file_drop::CARD_PAD_X))
            .pb(px(size::file_drop::CARD_PAD_BOTTOM))
            .rounded(px(size::file_drop::CARD_RADIUS))
            .border_1()
            .border_color(tokens.primary_light_darker())
            .bg(tokens.island())
            .shadow(crate::design::file_drop_card_shadow())
            .child(
                div()
                    .text_size(px(size::file_drop::TITLE_FONT))
                    .font_bold()
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .mt(px(size::file_drop::HINT_GAP))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_center()
                    .text_size(px(size::file_drop::HINT_FONT))
                    .text_color(tokens.surface_text())
                    .opacity(0.7)
                    .children(hint),
            );
        if state.kind == DropKind::Unknown {
            card = card.child(
                div()
                    .text_size(px(size::file_drop::SECONDARY_FONT))
                    .text_color(tokens.surface_text())
                    .opacity(0.45)
                    .child(SharedString::from(format!(
                        "({})",
                        t("fileDrop.libraryHint")
                    ))),
            );
        }
        card = card.child(self.file_drop_illustration(tokens, keeps));
        div()
            .id("file-drop-overlay")
            .test_support()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(size::file_drop::OVERLAY_PADDING))
            .bg(tokens.overlay_backdrop())
            .child(card)
            .into_any_element()
    }

    fn file_drop_illustration(&self, tokens: Tokens, keeps: bool) -> Div {
        let island = tokens.island_bg;
        let sheet = |x: f32, angle: f32, top: f32, stroke: u32, marks: &str| {
            let markup = format!(
                concat!(
                    r##"<svg viewBox="0 0 100 128" fill="none" stroke="#{stroke:06x}" "##,
                    r##"stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">"##,
                    r##"<path fill="#{fill:06x}" d="M16 3h43l28 28v81c0 7-4 12-12 12H16c-8 0-12-5-12-12V15C4 7 8 3 16 3Z"/>"##,
                    r##"<path d="M59 3v20c0 6 3 8 9 8h19"/>{marks}</svg>"##,
                ),
                stroke = stroke,
                fill = island,
                marks = marks,
            );
            svg()
                .data(markup.as_bytes())
                .absolute()
                .top(px(top))
                .left(px(0.0))
                .w(px(size::file_drop::SHEET_W))
                .h(px(size::file_drop::SHEET_H))
                .text_color(rgb(stroke))
                .with_transformation(
                    Transformation::rotate(radians(angle)).with_translation(point(px(x), px(0.0))),
                )
        };
        let (back_stroke, back_marks) = if keeps {
            (
                tokens.primary,
                r#"<path opacity="0.35" d="M24 61h43M24 77h43M24 93h26"/>"#,
            )
        } else {
            (
                tokens.primary_light_darker,
                r#"<path stroke-width="4" d="M18 44l24 24M42 44L18 68"/>"#,
            )
        };
        div()
            .relative()
            .w(px(size::file_drop::SHEET_W))
            .h(px(size::file_drop::ILLUSTRATION_HEIGHT))
            .mt(px(size::file_drop::ILLUSTRATION_GAP))
            .child(sheet(
                -24.0,
                (-18.0_f32).to_radians(),
                size::file_drop::SHEET_TOP_BACK,
                back_stroke,
                back_marks,
            ))
            .child(sheet(
                24.0,
                (12.0_f32).to_radians(),
                size::file_drop::SHEET_TOP,
                tokens.primary,
                r#"<path opacity="0.35" d="M24 61h43M24 77h43M24 93h26"/>"#,
            ))
    }

    fn render_cursor_hint(&self, tokens: Tokens) -> Option<gpui_kit::AnyElement> {
        let hint = self.cursor_hint.as_ref()?;
        let box_size = size::cursor_hint::ICON + size::cursor_hint::PADDING * 2.0;
        let (left, top) = position_beside_cursor(
            self.cursor_screen,
            (box_size as f64, box_size as f64),
            self.viewport,
            size::cursor_hint::GAP as f64,
        );
        Some(
            div()
                .id("cursor-hint")
                .test_support()
                .absolute()
                .left(px(left as f32))
                .top(px(top as f32))
                .flex()
                .items_center()
                .justify_center()
                .p(px(size::cursor_hint::PADDING))
                .rounded(px(size::RADIUS_MD))
                .bg(tokens.cursor_hint_bg())
                .shadow(crate::design::island_shadow())
                .child(crate::icons::icon(
                    hint.icon,
                    size::cursor_hint::ICON,
                    tokens.cursor_hint_fg(),
                ))
                .into_any_element(),
        )
    }

    fn render_toast(&self, tokens: Tokens, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        let toast = self.toast.as_ref()?;
        let mut card = div()
            .id("toast")
            .test_support()
            .relative()
            .min_w(px(size::toast::MIN_WIDTH))
            .max_w(px(size::toast::MAX_WIDTH))
            .px(px(size::toast::PAD_X))
            .py(px(size::toast::PAD_Y))
            .rounded(px(size::RADIUS_LG))
            .border_1()
            .border_color(tokens.surface_high())
            .bg(tokens.island())
            .shadow(crate::design::chip_ring_shadow(tokens))
            .text_size(px(size::toast::FONT))
            .text_color(tokens.surface_text())
            .child(
                div()
                    .px(px(size::toast::CLOSE_ICON + size::toast::CLOSE_PAD))
                    .text_center()
                    .child(SharedString::from(toast.message.clone())),
            );
        if toast.closable {
            card = card.child(
                div()
                    .id("toast-close")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .p(px(size::toast::CLOSE_PAD))
                    .text_color(tokens.surface_text())
                    .child(crate::icons::icon(
                        "CloseIcon",
                        size::toast::CLOSE_ICON,
                        tokens.surface_text(),
                    ))
                    .on_click(Self::act(cx, |this, _, _, _| this.dismiss_toast())),
            );
        }
        Some(
            div()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(size::toast::BOTTOM))
                .flex()
                .flex_col()
                .items_center()
                .child(card)
                .into_any_element(),
        )
    }

    fn render_eye_dropper(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        let state = self.eye_dropper.as_ref()?;
        let (left, top) = position_beside_cursor(
            state.at,
            (
                size::eye_dropper::PREVIEW as f64,
                size::eye_dropper::PREVIEW as f64,
            ),
            self.viewport,
            size::eye_dropper::PREVIEW_GAP as f64,
        );
        let ring = if is_dark_hex(&state.color) {
            rgb(0xffffff)
        } else {
            rgb(0x222222)
        };
        let preview = div()
            .absolute()
            .left(px(left as f32))
            .top(px(top as f32))
            .size(px(size::eye_dropper::PREVIEW))
            .rounded_full()
            .bg(crate::properties::parse_hex(&state.color))
            .border_1()
            .border_color(ring)
            .shadow(crate::design::eye_dropper_shadow());
        Some(
            div()
                .id("eye-dropper")
                .test_support()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .occlude()
                .on_mouse_move(Self::act(cx, |this, e: &MouseMoveEvent, _, _| {
                    this.update_eye_dropper((e.position.x.to_f64(), e.position.y.to_f64()));
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    Self::act(cx, |this, e: &MouseDownEvent, _, _| {
                        this.update_eye_dropper((e.position.x.to_f64(), e.position.y.to_f64()));
                        this.commit_eye_dropper();
                    }),
                )
                .child(preview)
                .into_any_element(),
        )
    }

    fn render_context_menu(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let pos = self.context_menu.unwrap_or(Point::zero());
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        let rule = || {
            div()
                .h(px(1.0))
                .my(px(4.0))
                .mx(px(4.0))
                .bg(tokens.separator())
        };

        chrome::dropdown(tokens)
            .left(px(pos.x as f32))
            .top(px(pos.y as f32))
            .child(
                chrome::menu_row("cm-copy", Some("copyIcon"), &t("ctx.copy"), tokens).on_click(
                    Self::act(cx, |this, _, _, _| {
                        this.on_copy();
                        this.context_menu = None;
                    }),
                ),
            )
            .child(
                chrome::menu_row("cm-paste", Some("clipboard"), &t("ctx.paste"), tokens).on_click(
                    Self::act(cx, |this, _, _, _| {
                        this.on_paste();
                        this.context_menu = None;
                    }),
                ),
            )
            .child(
                chrome::menu_row("cm-dup", Some("DuplicateIcon"), &t("ctx.duplicate"), tokens)
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.on_duplicate();
                        this.context_menu = None;
                    })),
            )
            .child(rule())
            .child(
                chrome::menu_row(
                    "cm-front",
                    Some("BringToFrontIcon"),
                    &t("ctx.bringFront"),
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.push_checkpoint();
                    this.document.bring_to_front();
                    this.context_menu = None;
                })),
            )
            .child(
                chrome::menu_row(
                    "cm-back",
                    Some("SendToBackIcon"),
                    &t("ctx.sendBack"),
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.push_checkpoint();
                    this.document.send_to_back();
                    this.context_menu = None;
                })),
            )
            .child(rule())
            .child(
                chrome::menu_row("cm-group", Some("GroupIcon"), &t("action.group"), tokens)
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.on_group();
                        this.context_menu = None;
                    })),
            )
            .child(
                chrome::menu_row(
                    "cm-ungroup",
                    Some("UngroupIcon"),
                    &t("action.ungroup"),
                    tokens,
                )
                .on_click(Self::act(cx, |this, _, _, _| {
                    this.on_ungroup();
                    this.context_menu = None;
                })),
            )
            .child(
                chrome::menu_row("cm-del", Some("TrashIcon"), &t("ctx.delete"), tokens).on_click(
                    Self::act(cx, |this, _, _, _| {
                        this.on_delete();
                        this.context_menu = None;
                    }),
                ),
            )
            .into_any_element()
    }
}

/// The palette's grouping, mirroring upstream `DEFAULT_CATEGORIES`. Upstream
/// renders these names raw (they are never run through `t()`), so the titles
/// stay English in every locale.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteCategory {
    App,
    Export,
    Editor,
    Tools,
    Elements,
}

impl PaletteCategory {
    fn title(self) -> &'static str {
        match self {
            Self::App => "App",
            Self::Export => "Export",
            Self::Editor => "Editor",
            Self::Tools => "Tools",
            Self::Elements => "Elements",
        }
    }

    /// Upstream's `getCategoryOrder` puts App first and the rest below it.
    /// Only the five categories we populate are listed.
    fn order(self) -> u8 {
        match self {
            Self::App => 1,
            Self::Export => 2,
            Self::Editor => 3,
            Self::Tools => 4,
            Self::Elements => 5,
        }
    }
}

struct PaletteCommand {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
    icon: &'static str,
    category: PaletteCategory,
    /// Extra search terms, mirroring upstream's `haystack` field.
    keywords: &'static str,
    /// Upstream's `predicate`, reduced to the checks this app can make.
    gate: PaletteGate,
    /// `None` marks a command upstream ships that this app has no handler for.
    /// The row stays listed, greyed out and unclickable, so the palette still
    /// matches upstream's full command set.
    action: Option<PaletteAction>,
}

/// Upstream's command `predicate`, reduced to the conditions this app can
/// evaluate. Failing the gate hides the row outright, the way upstream drops
/// commands before rendering them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteGate {
    /// Always listed.
    Always,
    /// Only once something is selected.
    Selection,
    /// Only once the canvas holds at least one element.
    Elements,
}

impl PaletteCommand {
    fn available(&self, editor: &Editor) -> bool {
        match self.gate {
            PaletteGate::Always => true,
            PaletteGate::Selection => !editor.document.selected.is_empty(),
            PaletteGate::Elements => editor.document.scene.non_deleted().next().is_some(),
        }
    }

    /// Rows without an action are upstream commands this app does not
    /// implement: they render greyed out and ignore clicks.
    fn implemented(&self) -> bool {
        self.action.is_some()
    }
}

#[derive(Clone, Copy)]
enum PaletteAction {
    Tool(Tool),
    Undo,
    Redo,
    Duplicate,
    Delete,
    SelectAll,
    Copy,
    Cut,
    Group,
    Ungroup,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFit,
    ZoomSelection,
    ToggleTheme,
    ToggleGrid,
    ToggleSnap,
    ToggleZen,
    ToggleView,
    ToggleStats,
    ToggleLock,
    ToggleLibrary,
    Save,
    ExportPng,
    CopyAsPng,
    CopyAsSvg,
    StrokePopup,
    FillPopup,
    CanvasBackground,
    NewCanvas,
    Find,
    Help,
}

impl PaletteAction {
    fn run(self, editor: &mut Editor) {
        let viewport = editor.viewport;
        match self {
            PaletteAction::Tool(tool) => editor.set_tool(tool),
            PaletteAction::Undo => editor.on_undo(),
            PaletteAction::Redo => editor.on_redo(),
            PaletteAction::Duplicate => editor.on_duplicate(),
            PaletteAction::Delete => editor.on_delete(),
            PaletteAction::SelectAll => editor.document.select_all(),
            PaletteAction::Copy => editor.on_copy(),
            PaletteAction::Cut => editor.on_cut(),
            PaletteAction::Group => editor.on_group(),
            PaletteAction::Ungroup => editor.on_ungroup(),
            PaletteAction::ZoomIn => editor.on_zoom_in(),
            PaletteAction::ZoomOut => editor.on_zoom_out(),
            PaletteAction::ZoomReset => editor.on_reset_zoom(),
            PaletteAction::ZoomFit => editor.document.zoom_to_fit(viewport, 40.0),
            PaletteAction::ZoomSelection => editor.document.zoom_to_selection(viewport, 40.0),
            PaletteAction::ToggleTheme => editor.on_toggle_theme(),
            PaletteAction::ToggleGrid => editor.on_toggle_grid(),
            PaletteAction::ToggleSnap => {
                editor.document.snap_to_objects = !editor.document.snap_to_objects;
            }
            PaletteAction::ToggleZen => editor.document.zen_mode = !editor.document.zen_mode,
            PaletteAction::ToggleView => editor.document.view_mode = !editor.document.view_mode,
            PaletteAction::ToggleStats => editor.document.show_stats = !editor.document.show_stats,
            PaletteAction::ToggleLock => editor.document.tool_locked = !editor.document.tool_locked,
            PaletteAction::ToggleLibrary => editor.on_toggle_library(),
            PaletteAction::Save => editor.pending_save_dialog = true,
            PaletteAction::ExportPng => {
                editor.pending_image_dialog = true;
            }
            // The clipboard needs `App`, which `run` does not carry, so these
            // two mark a flag the render pass picks up.
            PaletteAction::CopyAsPng => editor.pending_copy_png = true,
            PaletteAction::CopyAsSvg => editor.pending_copy_svg = true,
            PaletteAction::StrokePopup => editor.toggle_color_popup(ColorTarget::Stroke),
            PaletteAction::FillPopup => editor.toggle_color_popup(ColorTarget::Background),
            PaletteAction::CanvasBackground => {
                editor.set_menu_open(true);
                editor.toggle_color_popup(ColorTarget::CanvasBackground);
            }
            PaletteAction::NewCanvas => editor.show_clear_dialog = true,
            PaletteAction::Find => editor.open_find(),
            PaletteAction::Help => editor.open_help(),
        }
    }
}

// Rows transcribed from upstream CommandPalette.tsx, in its declaration
// order, with the host-only entries the released palette adds (live
// collaboration, share, export to Excalidraw+). Commands this app has no
// handler for carry `action: None` and render greyed out.
const PALETTE_COMMANDS: &[PaletteCommand] = &[
    // ---- App ----
    PaletteCommand {
        id: "app-theme",
        label: "labels.toggleTheme",
        shortcut: Some("Shift+Alt+D"),
        icon: "themeDarkIcon",
        category: PaletteCategory::App,
        keywords: "dark light mode theme appearance",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleTheme),
    },
    PaletteCommand {
        id: "app-library",
        label: "toolBar.library",
        shortcut: None,
        icon: "LibraryIcon",
        category: PaletteCategory::App,
        keywords: "library assets shapes panel",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleLibrary),
    },
    PaletteCommand {
        id: "app-search",
        label: "search.title",
        shortcut: Some("Ctrl+F"),
        icon: "searchIcon",
        category: PaletteCategory::App,
        keywords: "find search text locate",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Find),
    },
    PaletteCommand {
        id: "app-live-collab",
        label: "labels.liveCollaboration",
        shortcut: None,
        icon: "collabIcon",
        category: PaletteCategory::App,
        keywords: "live collaboration multiplayer session share",
        gate: PaletteGate::Always,
        action: None,
    },
    PaletteCommand {
        id: "app-share",
        label: "labels.share",
        shortcut: None,
        icon: "shareIcon",
        category: PaletteCategory::App,
        keywords: "share publish link",
        gate: PaletteGate::Always,
        action: None,
    },
    // ---- Export ----
    PaletteCommand {
        id: "export-image",
        label: "buttons.exportImage",
        shortcut: Some("Ctrl+Shift+E"),
        icon: "ExportImageIcon",
        category: PaletteCategory::Export,
        keywords: "export png svg image render download picture",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ExportPng),
    },
    PaletteCommand {
        id: "export-save",
        label: "exportDialog.disk_title",
        shortcut: Some("Ctrl+S"),
        icon: "ExportIcon",
        category: PaletteCategory::Export,
        keywords: "save file disk write download",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Save),
    },
    PaletteCommand {
        id: "export-copy-png",
        label: "labels.copyAsPng",
        shortcut: Some("Shift+Alt+C"),
        icon: "pngIcon",
        category: PaletteCategory::Export,
        keywords: "copy png image clipboard",
        gate: PaletteGate::Elements,
        action: Some(PaletteAction::CopyAsPng),
    },
    PaletteCommand {
        id: "export-copy-svg",
        label: "labels.copyAsSvg",
        shortcut: Some("Shift+Alt+C"),
        icon: "svgIcon",
        category: PaletteCategory::Export,
        keywords: "copy svg clipboard vector",
        gate: PaletteGate::Elements,
        action: Some(PaletteAction::CopyAsSvg),
    },
    PaletteCommand {
        id: "export-to-plus",
        label: "overwriteConfirm.action.excalidrawPlus.button",
        shortcut: None,
        icon: "exportToPlusIcon",
        category: PaletteCategory::Export,
        keywords: "excalidraw plus cloud export",
        gate: PaletteGate::Always,
        action: None,
    },
    // ---- Editor ----
    PaletteCommand {
        id: "editor-undo",
        label: "buttons.undo",
        shortcut: Some("Ctrl+Z"),
        icon: "UndoIcon",
        category: PaletteCategory::Editor,
        keywords: "undo revert back",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Undo),
    },
    PaletteCommand {
        id: "editor-redo",
        label: "buttons.redo",
        shortcut: Some("Ctrl+Shift+Z"),
        icon: "RedoIcon",
        category: PaletteCategory::Editor,
        keywords: "redo forward again",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Redo),
    },
    PaletteCommand {
        id: "editor-zoom-in",
        label: "buttons.zoomIn",
        shortcut: Some("Ctrl++"),
        icon: "ZoomInIcon",
        category: PaletteCategory::Editor,
        keywords: "zoom in enlarge bigger magnify",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ZoomIn),
    },
    PaletteCommand {
        id: "editor-zoom-out",
        label: "buttons.zoomOut",
        shortcut: Some("Ctrl+-"),
        icon: "ZoomOutIcon",
        category: PaletteCategory::Editor,
        keywords: "zoom out shrink smaller",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ZoomOut),
    },
    PaletteCommand {
        id: "editor-zoom-reset",
        label: "buttons.resetZoom",
        shortcut: Some("Ctrl+0"),
        icon: "resetZoom",
        category: PaletteCategory::Editor,
        keywords: "reset zoom actual size 100",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ZoomReset),
    },
    PaletteCommand {
        id: "editor-zoom-fit",
        label: "labels.zoomToFit",
        shortcut: Some("Shift+1"),
        icon: "zoomAreaIcon",
        category: PaletteCategory::Editor,
        keywords: "zoom to fit all elements page",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ZoomFit),
    },
    PaletteCommand {
        id: "editor-zen",
        label: "buttons.zenMode",
        shortcut: Some("Alt+Z"),
        icon: "eyeIcon",
        category: PaletteCategory::Editor,
        keywords: "zen mode focus hide ui",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleZen),
    },
    PaletteCommand {
        id: "editor-view-mode",
        label: "labels.viewMode",
        shortcut: Some("Alt+R"),
        icon: "eyeClosedIcon",
        category: PaletteCategory::Editor,
        keywords: "view mode read only preview",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleView),
    },
    PaletteCommand {
        id: "editor-grid",
        label: "labels.toggleGrid",
        shortcut: Some("Ctrl+'"),
        icon: "gridIcon",
        category: PaletteCategory::Editor,
        keywords: "grid background toggle",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleGrid),
    },
    PaletteCommand {
        id: "editor-snap",
        label: "buttons.objectsSnapMode",
        shortcut: Some("Alt+S"),
        icon: "magnetIcon",
        category: PaletteCategory::Editor,
        keywords: "snap objects magnet alignment",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleSnap),
    },
    PaletteCommand {
        id: "editor-show-hints",
        label: "labels.showHints",
        shortcut: None,
        icon: "messageCircleIcon",
        category: PaletteCategory::Editor,
        keywords: "hints labels tooltips show",
        gate: PaletteGate::Always,
        action: None,
    },
    PaletteCommand {
        id: "editor-shortcuts",
        label: "welcomeScreen.defaults.helpHint",
        shortcut: Some("?"),
        icon: "HelpIcon",
        category: PaletteCategory::Editor,
        keywords: "help shortcuts keyboard documentation",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Help),
    },
    PaletteCommand {
        id: "editor-select-all",
        label: "labels.selectAll",
        shortcut: Some("Ctrl+A"),
        icon: "selectAllIcon",
        category: PaletteCategory::Editor,
        keywords: "select all elements everything",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::SelectAll),
    },
    PaletteCommand {
        id: "editor-element-lock",
        label: "labels.elementLock.lock",
        shortcut: Some("Ctrl+Shift+L"),
        icon: "LockedIcon",
        category: PaletteCategory::Editor,
        keywords: "lock element freeze selection",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "editor-unlock-all",
        label: "labels.elementLock.unlockAll",
        shortcut: None,
        icon: "UnlockedIcon",
        category: PaletteCategory::Editor,
        keywords: "unlock all elements locked",
        gate: PaletteGate::Always,
        action: None,
    },
    PaletteCommand {
        id: "editor-stats",
        label: "stats.fullTitle",
        shortcut: Some("Alt+/"),
        icon: "slidersIcon",
        category: PaletteCategory::Editor,
        keywords: "stats properties canvas information",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleStats),
    },
    PaletteCommand {
        id: "editor-clear",
        label: "buttons.clearReset",
        shortcut: Some("Ctrl+Delete"),
        icon: "TrashIcon",
        category: PaletteCategory::Editor,
        keywords: "clear reset canvas delete destroy",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::NewCanvas),
    },
    PaletteCommand {
        id: "editor-canvas-background",
        label: "labels.canvasBackground",
        shortcut: None,
        icon: "bucketFillIcon",
        category: PaletteCategory::Editor,
        keywords: "canvas background color",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::CanvasBackground),
    },
    // ---- Tools ----
    PaletteCommand {
        id: "tool-hand",
        label: "toolBar.hand",
        shortcut: Some("H"),
        icon: "handIcon",
        category: PaletteCategory::Tools,
        keywords: "hand pan grab move canvas",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Hand)),
    },
    PaletteCommand {
        id: "tool-selection",
        label: "toolBar.selection",
        shortcut: Some("V"),
        icon: "SelectionIcon",
        category: PaletteCategory::Tools,
        keywords: "selection select pointer cursor move",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Selection)),
    },
    PaletteCommand {
        id: "tool-rectangle",
        label: "toolBar.rectangle",
        shortcut: Some("R"),
        icon: "RectangleIcon",
        category: PaletteCategory::Tools,
        keywords: "rectangle box square shape",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Rectangle)),
    },
    PaletteCommand {
        id: "tool-diamond",
        label: "toolBar.diamond",
        shortcut: Some("D"),
        icon: "DiamondIcon",
        category: PaletteCategory::Tools,
        keywords: "diamond rhombus shape",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Diamond)),
    },
    PaletteCommand {
        id: "tool-ellipse",
        label: "toolBar.ellipse",
        shortcut: Some("O"),
        icon: "EllipseIcon",
        category: PaletteCategory::Tools,
        keywords: "ellipse circle oval shape",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Ellipse)),
    },
    PaletteCommand {
        id: "tool-arrow",
        label: "toolBar.arrow",
        shortcut: Some("A"),
        icon: "ArrowIcon",
        category: PaletteCategory::Tools,
        keywords: "arrow connector line shape",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Arrow)),
    },
    PaletteCommand {
        id: "tool-line",
        label: "toolBar.line",
        shortcut: Some("L"),
        icon: "LineIcon",
        category: PaletteCategory::Tools,
        keywords: "line edge shape",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Line)),
    },
    PaletteCommand {
        id: "tool-freedraw",
        label: "toolBar.freedraw",
        shortcut: Some("P"),
        icon: "FreedrawIcon",
        category: PaletteCategory::Tools,
        keywords: "freedraw pencil draw sketch",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::FreeDraw)),
    },
    PaletteCommand {
        id: "tool-text",
        label: "toolBar.text",
        shortcut: Some("T"),
        icon: "TextIcon",
        category: PaletteCategory::Tools,
        keywords: "text label type words",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Text)),
    },
    PaletteCommand {
        id: "tool-stickynote",
        label: "toolBar.stickynote",
        shortcut: Some("N"),
        icon: "stickyNoteToolIcon",
        category: PaletteCategory::Tools,
        keywords: "sticky note post it memo",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::StickyNote)),
    },
    PaletteCommand {
        id: "tool-image",
        label: "toolBar.image",
        shortcut: Some("9"),
        icon: "ImageIcon",
        category: PaletteCategory::Tools,
        keywords: "image picture photo insert",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Image)),
    },
    PaletteCommand {
        id: "tool-eraser",
        label: "toolBar.eraser",
        shortcut: Some("E"),
        icon: "EraserIcon",
        category: PaletteCategory::Tools,
        keywords: "eraser erase delete remove",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Eraser)),
    },
    PaletteCommand {
        id: "tool-frame",
        label: "toolBar.frame",
        shortcut: Some("F"),
        icon: "frameToolIcon",
        category: PaletteCategory::Tools,
        keywords: "frame group container board",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Frame)),
    },
    PaletteCommand {
        id: "tool-laser",
        label: "toolBar.laser",
        shortcut: Some("K"),
        icon: "laserPointerToolIcon",
        category: PaletteCategory::Tools,
        keywords: "laser pointer present highlight",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::Tool(Tool::Laser)),
    },
    PaletteCommand {
        id: "tool-lock",
        label: "toolBar.lock",
        shortcut: Some("Q"),
        icon: "LockedIcon",
        category: PaletteCategory::Tools,
        keywords: "lock keep tool selected",
        gate: PaletteGate::Always,
        action: Some(PaletteAction::ToggleLock),
    },
    // ---- Elements ----
    PaletteCommand {
        id: "element-group",
        label: "labels.group",
        shortcut: Some("Ctrl+G"),
        icon: "GroupIcon",
        category: PaletteCategory::Elements,
        keywords: "group combine merge elements",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Group),
    },
    PaletteCommand {
        id: "element-ungroup",
        label: "labels.ungroup",
        shortcut: Some("Ctrl+Shift+G"),
        icon: "UngroupIcon",
        category: PaletteCategory::Elements,
        keywords: "ungroup split separate elements",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Ungroup),
    },
    PaletteCommand {
        id: "element-cut",
        label: "labels.cut",
        shortcut: Some("Ctrl+X"),
        icon: "cutIcon",
        category: PaletteCategory::Elements,
        keywords: "cut scissors remove clipboard",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Cut),
    },
    PaletteCommand {
        id: "element-copy",
        label: "labels.copy",
        shortcut: Some("Ctrl+C"),
        icon: "copyIcon",
        category: PaletteCategory::Elements,
        keywords: "copy duplicate clipboard",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Copy),
    },
    PaletteCommand {
        id: "element-delete",
        label: "labels.delete",
        shortcut: Some("Del"),
        icon: "TrashIcon",
        category: PaletteCategory::Elements,
        keywords: "delete remove trash erase",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Delete),
    },
    PaletteCommand {
        id: "element-wrap-in-frame",
        label: "labels.wrapSelectionInFrame",
        shortcut: Some("Ctrl+Shift+F"),
        icon: "frameToolIcon",
        category: PaletteCategory::Elements,
        keywords: "frame wrap selection container",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-toggle-wrap",
        label: "labels.toggleWrap",
        shortcut: Some("W"),
        icon: "boltIcon",
        category: PaletteCategory::Elements,
        keywords: "wrap text break line paragraph",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-copy-styles",
        label: "labels.copyStyles",
        shortcut: Some("Ctrl+Alt+C"),
        icon: "paintIcon",
        category: PaletteCategory::Elements,
        keywords: "copy styles format paint",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-paste-styles",
        label: "labels.pasteStyles",
        shortcut: Some("Ctrl+Alt+V"),
        icon: "paintIcon",
        category: PaletteCategory::Elements,
        keywords: "paste styles format paint",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-bring-to-front",
        label: "labels.bringToFront",
        shortcut: Some("Ctrl+Shift+]"),
        icon: "BringToFrontIcon",
        category: PaletteCategory::Elements,
        keywords: "bring to front order z index",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-bring-forward",
        label: "labels.bringForward",
        shortcut: Some("Ctrl+]"),
        icon: "BringForwardIcon",
        category: PaletteCategory::Elements,
        keywords: "bring forward order z index",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-send-backward",
        label: "labels.sendBackward",
        shortcut: Some("Ctrl+["),
        icon: "SendBackwardIcon",
        category: PaletteCategory::Elements,
        keywords: "send backward order z index",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-send-to-back",
        label: "labels.sendToBack",
        shortcut: Some("Ctrl+Shift+["),
        icon: "SendToBackIcon",
        category: PaletteCategory::Elements,
        keywords: "send to back order z index",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-align-top",
        label: "labels.alignTop",
        shortcut: Some("Ctrl+Shift+Up"),
        icon: "AlignTopIcon",
        category: PaletteCategory::Elements,
        keywords: "align top edge",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-align-bottom",
        label: "labels.alignBottom",
        shortcut: Some("Ctrl+Shift+Down"),
        icon: "AlignBottomIcon",
        category: PaletteCategory::Elements,
        keywords: "align bottom edge",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-align-left",
        label: "labels.alignLeft",
        shortcut: Some("Ctrl+Shift+Left"),
        icon: "AlignLeftIcon",
        category: PaletteCategory::Elements,
        keywords: "align left edge",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-align-right",
        label: "labels.alignRight",
        shortcut: Some("Ctrl+Shift+Right"),
        icon: "AlignRightIcon",
        category: PaletteCategory::Elements,
        keywords: "align right edge",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-center-vertically",
        label: "labels.centerVertically",
        shortcut: None,
        icon: "CenterVerticallyIcon",
        category: PaletteCategory::Elements,
        keywords: "align center vertical middle",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-center-horizontally",
        label: "labels.centerHorizontally",
        shortcut: None,
        icon: "CenterHorizontallyIcon",
        category: PaletteCategory::Elements,
        keywords: "align center horizontal middle",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-duplicate",
        label: "labels.duplicateSelection",
        shortcut: Some("Ctrl+D"),
        icon: "DuplicateIcon",
        category: PaletteCategory::Elements,
        keywords: "duplicate clone copy selection",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::Duplicate),
    },
    PaletteCommand {
        id: "element-flip-horizontal",
        label: "labels.flipHorizontal",
        shortcut: Some("Shift+H"),
        icon: "flipHorizontalIcon",
        category: PaletteCategory::Elements,
        keywords: "flip mirror horizontal",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-flip-vertical",
        label: "labels.flipVertical",
        shortcut: Some("Shift+V"),
        icon: "flipVerticalIcon",
        category: PaletteCategory::Elements,
        keywords: "flip mirror vertical",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-zoom-fit",
        label: "labels.zoomToFitSelection",
        shortcut: Some("Shift+2"),
        icon: "zoomAreaIcon",
        category: PaletteCategory::Elements,
        keywords: "zoom to fit selection selected elements",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::ZoomSelection),
    },
    PaletteCommand {
        id: "element-zoom-fit-viewport",
        label: "labels.zoomToFitViewport",
        shortcut: Some("Shift+3"),
        icon: "zoomAreaIcon",
        category: PaletteCategory::Elements,
        keywords: "zoom to fit selection viewport",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-font-increase",
        label: "labels.increaseFontSize",
        shortcut: Some("Ctrl+Shift+>"),
        icon: "fontSizeIcon",
        category: PaletteCategory::Elements,
        keywords: "increase font size text bigger",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-font-decrease",
        label: "labels.decreaseFontSize",
        shortcut: Some("Ctrl+Shift+<"),
        icon: "fontSizeIcon",
        category: PaletteCategory::Elements,
        keywords: "decrease font size text smaller",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-line-editor",
        label: "labels.lineEditor.edit",
        shortcut: Some("Ctrl+Enter"),
        icon: "polygonIcon",
        category: PaletteCategory::Elements,
        keywords: "edit line arrow points",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-image-crop",
        label: "labels.imageCropping",
        shortcut: Some("Enter"),
        icon: "cropIcon",
        category: PaletteCategory::Elements,
        keywords: "crop image cropping",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-toggle-polygon",
        label: "labels.polygon.convertToPolygon",
        shortcut: None,
        icon: "polygonIcon",
        category: PaletteCategory::Elements,
        keywords: "polygon convert line shape",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-link",
        label: "labels.link.label",
        shortcut: None,
        icon: "LinkIcon",
        category: PaletteCategory::Elements,
        keywords: "link hyperlink url element",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-copy-link",
        label: "labels.copyElementLink",
        shortcut: None,
        icon: "copyIcon",
        category: PaletteCategory::Elements,
        keywords: "copy element link url clipboard",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-link-to",
        label: "labels.linkToElement",
        shortcut: None,
        icon: "elementLinkIcon",
        category: PaletteCategory::Elements,
        keywords: "link to element connect",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-shape-switch",
        label: "labels.shapeSwitch",
        shortcut: None,
        icon: "boltIcon",
        category: PaletteCategory::Elements,
        keywords: "shape switch convert",
        gate: PaletteGate::Selection,
        action: None,
    },
    PaletteCommand {
        id: "element-change-stroke",
        label: "labels.changeStroke",
        shortcut: None,
        icon: "bucketFillIcon",
        category: PaletteCategory::Elements,
        keywords: "color outline stroke",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::StrokePopup),
    },
    PaletteCommand {
        id: "element-change-background",
        label: "labels.changeBackground",
        shortcut: None,
        icon: "bucketFillIcon",
        category: PaletteCategory::Elements,
        keywords: "color fill background",
        gate: PaletteGate::Selection,
        action: Some(PaletteAction::FillPopup),
    },
];

/// A help-dialog row: the label key, the shortcut groups, and whether the
/// groups are alternatives (`HELP_OR`) or one sequence (`HELP_SEQ`).
type HelpRow = (&'static str, &'static [&'static str], bool);

/// A help-dialog island: its caption key and its rows.
type HelpIsland = (&'static str, &'static [HelpRow]);

/// Shortcut groups read as alternatives ("Ctrl or Shift+Enter").
const HELP_OR: bool = true;

/// Shortcut groups read as a single sequence ("A, click, click, click").
const HELP_SEQ: bool = false;

// Rows transcribed from upstream HelpDialog.tsx. Keys are upstream's own so the
// two dialogs stay textually identical; a chip that names a `helpDialog.*` key
// (e.g. `helpDialog.click`) is resolved at render time, everything else is a
// literal shortcut.
const HELP_TOOLS: HelpIsland = (
    "helpDialog.tools",
    &[
        ("toolBar.hand", &["H"], HELP_OR),
        ("toolBar.selection", &["V", "1"], HELP_OR),
        ("toolBar.rectangle", &["R", "2"], HELP_OR),
        ("toolBar.diamond", &["D", "3"], HELP_OR),
        ("toolBar.ellipse", &["O", "4"], HELP_OR),
        ("toolBar.arrow", &["A", "5"], HELP_OR),
        ("toolBar.line", &["L", "6"], HELP_OR),
        ("toolBar.freedraw", &["P", "7"], HELP_OR),
        ("toolBar.text", &["T", "8"], HELP_OR),
        ("toolBar.stickynote", &["N"], HELP_OR),
        ("toolBar.image", &["9"], HELP_OR),
        ("toolBar.eraser", &["E", "0"], HELP_OR),
        ("toolBar.frame", &["F"], HELP_OR),
        ("toolBar.laser", &["K"], HELP_OR),
        ("toolBar.bucketfill", &["B"], HELP_OR),
        ("labels.eyeDropper", &["I", "Shift+S", "Shift+G"], HELP_OR),
        ("helpDialog.editLineArrowPoints", &["Ctrl+Enter"], HELP_OR),
        ("helpDialog.editText", &["Enter"], HELP_OR),
        ("helpDialog.textNewLine", &["Enter", "Shift+Enter"], HELP_OR),
        ("helpDialog.textFinish", &["Esc", "Ctrl+Enter"], HELP_OR),
        (
            "helpDialog.curvedArrow",
            &[
                "A",
                "helpDialog.click",
                "helpDialog.click",
                "helpDialog.click",
            ],
            HELP_SEQ,
        ),
        (
            "helpDialog.curvedLine",
            &[
                "L",
                "helpDialog.click",
                "helpDialog.click",
                "helpDialog.click",
            ],
            HELP_SEQ,
        ),
        (
            "helpDialog.cropStart",
            &["helpDialog.doubleClick", "Enter"],
            HELP_OR,
        ),
        ("helpDialog.cropFinish", &["Enter", "Esc"], HELP_OR),
        ("toolBar.lock", &["Q"], HELP_OR),
        ("helpDialog.preventBinding", &["Ctrl"], HELP_OR),
        ("toolBar.link", &["Ctrl+K"], HELP_OR),
        ("toolBar.convertElementType", &["Tab", "Shift+Tab"], HELP_OR),
    ],
);

const HELP_VIEW: HelpIsland = (
    "helpDialog.view",
    &[
        ("buttons.zoomIn", &["Ctrl++"], HELP_OR),
        ("buttons.zoomOut", &["Ctrl+-"], HELP_OR),
        ("buttons.resetZoom", &["Ctrl+0"], HELP_OR),
        ("helpDialog.zoomToFit", &["Shift+1"], HELP_OR),
        ("helpDialog.zoomToSelection", &["Shift+2"], HELP_OR),
        ("helpDialog.movePageUpDown", &["PgUp/PgDn"], HELP_OR),
        (
            "helpDialog.movePageLeftRight",
            &["Shift+PgUp/PgDn"],
            HELP_OR,
        ),
        ("buttons.zenMode", &["Alt+Z"], HELP_OR),
        ("buttons.objectsSnapMode", &["Alt+S"], HELP_OR),
        ("labels.toggleGrid", &["Ctrl+'"], HELP_OR),
        ("labels.viewMode", &["Alt+R"], HELP_OR),
        ("labels.toggleTheme", &["Alt+Shift+D"], HELP_OR),
        ("stats.fullTitle", &["Alt+/"], HELP_OR),
        ("search.title", &["Ctrl+F"], HELP_OR),
        ("commandPalette.title", &["Ctrl+/", "Ctrl+Shift+P"], HELP_OR),
    ],
);

const HELP_EDITOR: HelpIsland = (
    "helpDialog.editor",
    &[
        ("helpDialog.createFlowchart", &["Ctrl+Arrow Key"], HELP_OR),
        ("helpDialog.navigateFlowchart", &["Alt+Arrow Key"], HELP_OR),
        (
            "labels.moveCanvas",
            &["Space+helpDialog.drag", "Wheel+helpDialog.drag"],
            HELP_OR,
        ),
        ("buttons.clearReset", &["Ctrl+Delete"], HELP_OR),
        ("labels.delete", &["Delete"], HELP_OR),
        ("labels.cut", &["Ctrl+X"], HELP_OR),
        ("labels.copy", &["Ctrl+C"], HELP_OR),
        ("labels.paste", &["Ctrl+V"], HELP_OR),
        ("labels.pasteAsPlaintext", &["Ctrl+Shift+V"], HELP_OR),
        ("labels.selectAll", &["Ctrl+A"], HELP_OR),
        ("labels.multiSelect", &["Shift+helpDialog.click"], HELP_OR),
        ("helpDialog.deepSelect", &["Ctrl+helpDialog.click"], HELP_OR),
        (
            "helpDialog.deepBoxSelect",
            &["Ctrl+helpDialog.drag"],
            HELP_OR,
        ),
        ("labels.copyAsPng", &["Shift+Alt+C"], HELP_OR),
        ("labels.copyStyles", &["Ctrl+Alt+C"], HELP_OR),
        ("labels.pasteStyles", &["Ctrl+Alt+V"], HELP_OR),
        ("labels.sendToBack", &["Ctrl+Shift+["], HELP_OR),
        ("labels.bringToFront", &["Ctrl+Shift+]"], HELP_OR),
        ("labels.sendBackward", &["Ctrl+["], HELP_OR),
        ("labels.bringForward", &["Ctrl+]"], HELP_OR),
        ("labels.alignTop", &["Ctrl+Shift+Up"], HELP_OR),
        ("labels.alignBottom", &["Ctrl+Shift+Down"], HELP_OR),
        ("labels.alignLeft", &["Ctrl+Shift+Left"], HELP_OR),
        ("labels.alignRight", &["Ctrl+Shift+Right"], HELP_OR),
        (
            "labels.duplicateSelection",
            &["Ctrl+D", "Alt+helpDialog.drag"],
            HELP_OR,
        ),
        ("helpDialog.toggleElementLock", &["Ctrl+Shift+L"], HELP_OR),
        ("buttons.undo", &["Ctrl+Z"], HELP_OR),
        ("buttons.redo", &["Ctrl+Shift+Z"], HELP_OR),
        ("labels.group", &["Ctrl+G"], HELP_OR),
        ("labels.ungroup", &["Ctrl+Shift+G"], HELP_OR),
        ("labels.toggleWrap", &["W"], HELP_OR),
        ("labels.flipHorizontal", &["Shift+H"], HELP_OR),
        ("labels.flipVertical", &["Shift+V"], HELP_OR),
        ("labels.showStroke", &["S"], HELP_OR),
        ("labels.showBackground", &["G"], HELP_OR),
        ("labels.showFonts", &["Shift+F"], HELP_OR),
        ("labels.decreaseFontSize", &["Ctrl+Shift+<"], HELP_OR),
        ("labels.increaseFontSize", &["Ctrl+Shift+>"], HELP_OR),
    ],
);

const HELP_ISLANDS: [HelpIsland; 3] = [HELP_TOOLS, HELP_VIEW, HELP_EDITOR];

/// Splits a shortcut into its individual key chips, mirroring upstream's
/// `endsWith("++")` special case so `Ctrl++` renders as `Ctrl` `+`.
fn help_chip_parts(shortcut: &str) -> Vec<String> {
    match shortcut.strip_suffix("++") {
        Some(stem) => {
            let mut parts: Vec<String> = stem.split('+').map(|s| s.to_string()).collect();
            parts.push("+".to_string());
            parts
        }
        None => shortcut.split('+').map(|s| s.to_string()).collect(),
    }
}

fn find_nav_button(
    id: &'static str,
    glyph: &'static str,
    forward: bool,
    tokens: crate::design::Tokens,
    cx: &mut Context<Editor>,
) -> impl IntoElement + use<> {
    div()
        .id(id)
        .test_support()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size::BUTTON))
        .rounded(px(size::RADIUS_MD))
        .text_color(tokens.surface_text())
        .hover(move |s| s.bg(tokens.hover()))
        .child(SharedString::from(glyph.to_owned()))
        .on_click(Editor::act(cx, move |this, _, _, _| {
            this.step_find(forward)
        }))
}

impl Editor {
    fn render_palette(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        use crate::design::size::command_palette as p;

        let sections = self.palette_sections();
        // Upstream pins the dialog to `height: 100%` under a 750px cap on
        // desktop widths, so the list always owns the leftover space.
        let height = (self.viewport.1 as f32 - 2.0 * p::MODAL_PADDING).clamp(200.0, p::MAX_HEIGHT);

        // The search field: upstream's TextField in its focused state, which is
        // why it carries the brand-active border. It takes a single 12px inset
        // because the component is rendered without a leading icon.
        let field_text = if self.palette_query.is_empty() {
            t("commandPalette.search.placeholder")
        } else {
            self.palette_query.clone()
        };
        let field_color = if self.palette_query.is_empty() {
            tokens.field_placeholder()
        } else {
            tokens.popup_text()
        };
        let field = div()
            .id("palette-query")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .h(px(p::FIELD_HEIGHT))
            .px(px(p::FIELD_PAD_X))
            .bg(tokens.surface_low())
            .border_1()
            .border_color(tokens.brand_active())
            .rounded(px(p::FIELD_RADIUS))
            .text_size(px(p::FIELD_FONT))
            .text_color(field_color)
            .child(div().truncate().child(SharedString::from(field_text)));

        // The centred hint strip that upstream paints under the field.
        let hint = |keys: &[&str], desc: &str| {
            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(p::HINT_GAP))
                .h(px(p::HINT_HEIGHT));
            for key in keys {
                row = row.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px(px(p::HINT_KEY_PAD_X))
                        .h(px(p::HINT_HEIGHT))
                        .rounded(px(p::HINT_KEY_RADIUS))
                        .bg(tokens.selected_bg())
                        .text_size(px(p::HINT_FONT))
                        .text_color(tokens.popup_text())
                        .child(SharedString::from(*key)),
                );
            }
            row.child(
                div()
                    .ml(px(p::HINT_DESC_MARGIN))
                    .text_size(px(p::HINT_FONT))
                    .text_color(tokens.hint_desc())
                    .child(SharedString::from(desc.to_owned())),
            )
        };
        let hints = div()
            .id("palette-hints")
            .test_support()
            .flex()
            .flex_row()
            .justify_center()
            .gap(px(p::HINTS_GAP))
            .mt(px(p::HINTS_MARGIN_TOP))
            .child(hint(
                &["\u{2191}\u{2193}"],
                &t("commandPalette.shortcuts.select"),
            ))
            .child(hint(&["\u{21b5}"], &t("commandPalette.shortcuts.confirm")))
            .child(hint(&["Esc"], &t("commandPalette.shortcuts.close")));

        // One chip per key, splitting "Ctrl+Shift+E" the way upstream's
        // `Shortcut` component does.
        let shortcut_keys = |shortcut: &str| {
            let mut keys = div()
                .flex_shrink_0()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(p::KEY_GAP));
            for part in help_chip_parts(shortcut) {
                keys = keys.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px(px(p::KEY_PAD_X))
                        .h(px(p::KEY_HEIGHT))
                        .rounded(px(p::KEY_RADIUS))
                        .bg(tokens.selected_bg())
                        .text_size(px(p::KEY_FONT))
                        .text_color(tokens.popup_text())
                        .child(SharedString::from(part)),
                );
            }
            keys
        };

        let mut sections_ui: Vec<gpui_kit::AnyElement> = Vec::new();
        let mut index = 0usize;
        for (title, items) in &sections {
            let recents = *title == "commandPalette.recents";
            // Upstream prints the Recents title first and tags the history icon
            // onto its right edge with a 6px margin.
            let mut heading = div()
                .flex()
                .flex_row()
                .items_center()
                .mb(px(p::CATEGORY_MARGIN_BOTTOM))
                .text_size(px(p::CATEGORY_FONT))
                .font_semibold()
                .text_color(tokens.popup_text())
                .child(SharedString::from(if recents {
                    t("commandPalette.recents")
                } else {
                    (*title).to_owned()
                }));
            if recents {
                heading = heading.child(
                    div()
                        .ml(px(p::RECENTS_ICON_GAP))
                        .flex()
                        .items_center()
                        .child(crate::icons::icon(
                            "historyCommandIcon",
                            p::CATEGORY_FONT,
                            tokens.popup_text(),
                        )),
                );
            }

            let mut rows = div().flex().flex_col();
            for command in items {
                let selected = index == self.palette_index;
                // Commands upstream ships without a local handler keep their
                // row, greyed out and inert, the way upstream styles
                // `command-palette-item-disabled`.
                let implemented = command.implemented();
                let mut row = div()
                    .id(SharedString::from(format!("palette-row-{index}")))
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(p::ITEM_HEIGHT))
                    .px(px(p::ITEM_PAD_X))
                    .rounded(px(p::ITEM_RADIUS))
                    .text_color(tokens.popup_text())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(p::ITEM_NAME_GAP))
                            .overflow_hidden()
                            .child(crate::icons::icon(
                                command.icon,
                                p::ITEM_ICON,
                                tokens.popup_text(),
                            ))
                            .child(div().truncate().child(SharedString::from(t(command.label)))),
                    );
                if let Some(shortcut) = command.shortcut {
                    row = row.child(shortcut_keys(shortcut));
                }
                if !implemented {
                    row = row.opacity(p::DISABLED_OPACITY).cursor_not_allowed();
                } else {
                    row = row.cursor_pointer();
                    if selected {
                        row = row.bg(tokens.surface_mid());
                    } else {
                        row = row.hover(move |style| style.bg(tokens.surface_mid()));
                    }
                }
                let clicked = index;
                rows = rows.child(if implemented {
                    row.on_click(Self::act(cx, move |this, _, _, _| {
                        this.palette_index = clicked;
                        this.run_palette_selection();
                    }))
                } else {
                    // Upstream's disabled item still binds a no-op handler, so
                    // the click is swallowed instead of closing the dialog.
                    row.on_click(|_, _, _| {})
                });
                index += 1;
            }

            sections_ui.push(
                div()
                    .id(SharedString::from(format!("palette-section-{title}")))
                    .test_support()
                    .flex()
                    .flex_col()
                    .pr(px(p::CATEGORY_MARGIN_RIGHT))
                    .py(px(p::CATEGORY_PAD_Y))
                    .child(heading)
                    .child(rows)
                    .into_any_element(),
            );
        }

        let content: gpui_kit::AnyElement = if sections_ui.is_empty() {
            div()
                .id("palette-empty")
                .test_support()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(p::ITEM_NAME_GAP))
                .mt(px(p::NO_MATCH_MARGIN_TOP))
                .text_size(px(p::FIELD_FONT))
                .text_color(tokens.popup_text())
                .child(crate::icons::icon(
                    "searchIcon",
                    p::ITEM_ICON,
                    tokens.popup_text(),
                ))
                .child(SharedString::from(t("commandPalette.search.noMatch")))
                .into_any_element()
        } else {
            let mut list = div()
                .id("palette-commands")
                .test_support()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .mt(px(p::COMMANDS_MARGIN_TOP))
                .overflow_y_scroll();
            for section in sections_ui {
                list = list.child(section);
            }
            list.into_any_element()
        };

        let panel = div()
            .id("palette-panel")
            .test_support()
            .flex()
            .flex_col()
            .w(px(p::WIDTH))
            .max_w_full()
            .h(px(height))
            .p(px(p::ISLAND_PADDING))
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(p::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .child(field)
            .child(hints)
            .child(content);

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("palette-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| this.show_palette = false)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(p::MODAL_PADDING))
                    .child(panel),
            )
            .into_any_element()
    }

    fn render_find(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        let width = 340.0_f32;
        let left =
            (self.viewport.0 as f32 - width - size::CONTAINER_PADDING).max(size::CONTAINER_PADDING);
        let count = self.find_matches.len();
        let position = if count == 0 { 0 } else { self.find_index + 1 };
        let counter = format!("{position}/{count}");

        let query_color = if self.find_query.is_empty() {
            tokens.shortcut_text()
        } else {
            tokens.surface_text()
        };
        let query_text = if self.find_query.is_empty() {
            t("find.placeholder")
        } else {
            self.find_query.clone()
        };

        let body = if count == 0 && !self.find_query.is_empty() {
            div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(13.0))
                .text_color(tokens.shortcut_text())
                .child(SharedString::from(t("find.noResults")))
        } else {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .px(px(4.0))
                .child(find_nav_button("find-prev", "\u{2039}", false, tokens, cx))
                .child(find_nav_button("find-next", "\u{203a}", true, tokens, cx))
                .child(
                    div()
                        .flex_1()
                        .text_center()
                        .text_size(px(13.0))
                        .text_color(tokens.surface_text())
                        .child(SharedString::from(counter)),
                )
                .child(
                    div()
                        .id("find-close")
                        .test_support()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(size::BUTTON))
                        .rounded(px(size::RADIUS_MD))
                        .hover(move |s| s.bg(tokens.hover()))
                        .child(crate::icons::icon(
                            "CloseIcon",
                            size::ICON,
                            tokens.surface_text(),
                        ))
                        .on_click(Self::act(cx, |this, _, _, _| this.show_find = false)),
                )
        };

        chrome::dropdown(tokens)
            .left(px(left))
            .top(px(80.0))
            .w(px(width))
            .min_w(px(width))
            .child(
                div()
                    .id("find-query")
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(8.0))
                    .text_size(px(15.0))
                    .text_color(query_color)
                    .child(crate::icons::icon(
                        "searchIcon",
                        size::ICON,
                        tokens.shortcut_text(),
                    ))
                    .child(div().flex_1().child(SharedString::from(query_text))),
            )
            .child(body)
            .into_any_element()
    }

    fn render_help(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);
        use crate::design::size::help_dialog as d;

        // A shortcut key chip. Upstream colours these with
        // `--color-primary-light` and resolves any `helpDialog.*` entry
        // (`click`, `drag`, …) through the translator.
        let key_chip = |part: &str| {
            let text = if part.starts_with("helpDialog.") {
                t(part)
            } else {
                part.to_string()
            };
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(d::KEY_PAD))
                .py(px(d::KEY_PAD))
                .rounded(px(d::KEY_RADIUS))
                .bg(tokens.selected_bg())
                .text_size(px(d::KEY_FONT))
                .text_color(tokens.surface_text())
                .child(SharedString::from(text))
        };

        let mut islands: Vec<gpui_kit::AnyElement> = Vec::new();
        for &(caption, rows) in HELP_ISLANDS.iter() {
            let mut content = div()
                .flex()
                .flex_col()
                .overflow_hidden()
                .border_1()
                .border_color(tokens.dialog_border())
                .rounded(px(d::ISLAND_RADIUS));
            let last = rows.len().saturating_sub(1);
            for (index, &(label, shortcuts, is_or)) in rows.iter().enumerate() {
                let mut keys = div()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(d::KEY_GAP));
                for (group_index, shortcut) in shortcuts.iter().enumerate() {
                    if group_index > 0 && is_or {
                        keys = keys.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(d::SHORTCUT_FONT))
                                .text_color(tokens.surface_text())
                                .child(SharedString::from(t("helpDialog.or"))),
                        );
                    }
                    for part in help_chip_parts(shortcut) {
                        keys = keys.child(key_chip(&part));
                    }
                }
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(d::SHORTCUT_GAP))
                    .w_full()
                    .px(px(d::SHORTCUT_PAD_X))
                    .py(px(d::SHORTCUT_PAD_Y))
                    .text_size(px(d::SHORTCUT_FONT))
                    .text_color(tokens.surface_text())
                    .child(div().flex_1().child(SharedString::from(t(label))))
                    .child(keys);
                if index != last {
                    row = row.border_b_1().border_color(tokens.dialog_border());
                }
                content = content.child(row);
            }
            islands.push(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .mb(px(d::ISLAND_TITLE_GAP))
                            .text_size(px(d::ISLAND_TITLE_FONT))
                            .font_bold()
                            .text_color(tokens.surface_text())
                            .child(SharedString::from(t(caption))),
                    )
                    .child(content)
                    .into_any_element(),
            );
        }

        // Upstream lays the islands on a two-column grid: tools and view stack
        // in the left column while editor spans both rows on the right.
        let columns = div()
            .id("help-columns")
            .test_support()
            .flex()
            .flex_row()
            .gap(px(d::GRID_COL_GAP))
            .w_full()
            .child(
                div()
                    .id("help-col-left")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(d::GRID_ROW_GAP))
                    .child(islands.remove(0))
                    .child(islands.remove(0)),
            )
            .child(
                div()
                    .id("help-col-right")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(islands.remove(0)),
            );

        let body = div()
            .id("help-body")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(
                div()
                    .my(px(d::SECTION_MARGIN))
                    .text_size(px(d::SECTION_FONT))
                    .font_bold()
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("helpDialog.shortcuts"))),
            )
            .child(columns);

        let content = div()
            .relative()
            .flex()
            .flex_col()
            .w(px(d::WIDTH))
            .max_w_full()
            .max_h(px(
                (self.viewport.1 as f32 - 2.0 * d::MODAL_PADDING).max(200.0)
            ))
            .p(px(d::CONTENT_PADDING))
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .child(
                div()
                    .pb(px(d::TITLE_PAD_BOTTOM))
                    .mb(px(d::TITLE_MARGIN_BOTTOM))
                    .border_b_1()
                    .border_color(tokens.dialog_border())
                    .text_size(px(d::TITLE_FONT))
                    .font_bold()
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("helpDialog.title"))),
            )
            .child(body)
            .child(
                div()
                    .id("help-close")
                    .test_support()
                    .absolute()
                    .top(px(d::CLOSE_TOP))
                    .right(px(d::CLOSE_RIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(crate::icons::icon(
                        "CloseIcon",
                        d::CLOSE_ICON,
                        rgb(0xb8b8b8),
                    ))
                    .on_click(Self::act(cx, |this, _, _, _| this.show_help = false)),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("help-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| this.show_help = false)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(d::MODAL_PADDING))
                    .child(content),
            )
            .into_any_element()
    }

    /// Upstream ConfirmDialog for `clearCanvas`: a small dialog with the title
    /// over a rule, the alert body, then the cancel/confirm pair on the right.
    fn render_clear_canvas_dialog(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);

        use crate::design::size::clear_dialog as d;

        let hover_bg = tokens.hover();
        let danger_bg = tokens.danger();
        let danger_hover_bg = tokens.danger_hover();

        let cancel = div()
            .id("clear-dialog-cancel")
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .h(px(d::BUTTON_H))
            .px(px(d::BUTTON_PAD_X))
            .rounded(px(d::BUTTON_RADIUS))
            .border_1()
            .border_color(tokens.border())
            .text_size(px(d::BUTTON_FONT))
            .font_semibold()
            .text_color(tokens.surface_text())
            .hover(move |s| s.bg(hover_bg))
            .child(SharedString::from(t("buttons.cancel")))
            .on_click(Self::act(cx, |this, _, _, _| {
                this.show_clear_dialog = false
            }));

        let confirm = div()
            .id("clear-dialog-confirm")
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .h(px(d::BUTTON_H))
            .px(px(d::BUTTON_PAD_X))
            .rounded(px(d::BUTTON_RADIUS))
            .border_1()
            .border_color(danger_bg)
            .bg(danger_bg)
            .text_size(px(d::BUTTON_FONT))
            .font_semibold()
            .text_color(tokens.surface_lowest())
            .hover(move |s| s.bg(danger_hover_bg))
            .child(SharedString::from(t("buttons.confirm")))
            .on_click(Self::act(cx, |this, _, _, _| this.confirm_clear_canvas()));

        let content = div()
            .flex()
            .flex_col()
            .w(px(d::WIDTH))
            .max_w_full()
            .p(px(d::MODAL_PADDING))
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .child(
                div()
                    .pb(px(d::TITLE_PAD_BOTTOM))
                    .text_size(px(d::TITLE_FONT))
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("clearCanvasDialog.title"))),
            )
            .child(div().h(px(1.0)).w_full().bg(tokens.dialog_border()))
            .child(
                div()
                    .mt(px(d::TITLE_MARGIN_BOTTOM))
                    .text_size(px(d::BODY_FONT))
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("alerts.clearReset"))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(d::BUTTON_GAP))
                    .w_full()
                    .mt(px(d::BUTTON_TOP))
                    .child(cancel)
                    .child(confirm),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("clear-dialog-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.show_clear_dialog = false
                    })),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(d::MODAL_PADDING))
                    .child(content),
            )
            .into_any_element()
    }

    fn render_save_dialog(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);

        use crate::design::size::save_dialog as d;

        // Split the field into the runs the caret and selection divide it
        // into. Centering the runs as a flex row keeps the text centred
        // without measuring glyphs, which is enough for a single-line field.
        let name = self.project_name.as_str();
        let name_len = name.len();
        let caret = self.name_caret.min(name_len);
        let (sel_start, sel_end) = self.save_name_range();
        let caret_color = tokens.surface_text();
        let caret_element = || {
            div()
                .flex_shrink_0()
                .w(px(1.0))
                .h(px(d::NAME_FIELD_FONT))
                .bg(caret_color)
                .with_animation(
                    "save-name-caret",
                    Animation::new(std::time::Duration::from_millis(1000)).repeat_synced(),
                    |el, progress| el.opacity(if progress < 0.5 { 1.0 } else { 0.0 }),
                )
                .into_any_element()
        };

        let mut cuts = [0usize, name_len, caret, sel_start, sel_end];
        cuts.sort_unstable();

        let mut runs: Vec<gpui_kit::AnyElement> = Vec::new();
        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start >= end {
                continue;
            }
            if start == caret {
                runs.push(caret_element());
            }
            let mut run = div().child(SharedString::from(name[start..end].to_string()));
            if start < sel_end && sel_start < end {
                run = run
                    .bg(tokens.selected_bg())
                    .text_color(tokens.selected_fg());
            }
            runs.push(run.into_any_element());
        }
        if caret == name_len {
            runs.push(caret_element());
        }

        let project_name = div()
            .flex()
            .flex_col()
            .items_start()
            .my(px(d::NAME_MARGIN))
            .child(
                div()
                    .my(px(d::NAME_LABEL_MARGIN))
                    .font_bold()
                    .child(SharedString::from(format!("{}:", t("labels.fileTitle")))),
            )
            .child(
                div()
                    .id("save-filename")
                    .test_support()
                    .aria_label(SharedString::from(t("labels.fileTitle")))
                    .aria_value(SharedString::from(self.project_name_value()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .w(px(d::NAME_FIELD_W))
                    .h(px(d::NAME_FIELD_H))
                    .px(px(d::NAME_FIELD_PAD))
                    .border(px(d::NAME_FIELD_BORDER))
                    .border_color(tokens.input_border())
                    .hover(move |s| s.border_color(tokens.brand_hover()))
                    .rounded(px(d::NAME_FIELD_RADIUS))
                    .bg(tokens.surface_lowest())
                    .text_size(px(d::NAME_FIELD_FONT))
                    .text_center()
                    .text_color(tokens.surface_text())
                    .children(runs),
            );

        let card = div()
            .flex()
            .flex_col()
            .items_center()
            .max_w(px(d::CARD_MAX_WIDTH))
            .m(px(d::CARD_MARGIN))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .p(px(d::CARD_ICON_PAD))
                    .rounded_full()
                    .bg(rgb(CARD_LIME))
                    .child(crate::icons::icon(
                        "exportToFileIcon",
                        d::CARD_ICON,
                        rgb(0xffffff),
                    )),
            )
            .child(
                div()
                    .my(px(d::CARD_TITLE_MARGIN))
                    .text_size(px(d::CARD_TITLE_FONT))
                    .font_bold()
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("exportDialog.disk_title"))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .w_full()
                    .min_h(px(d::CARD_DETAILS_MIN_H))
                    .px(px(d::CARD_DETAILS_PAD))
                    .text_size(px(d::CARD_DETAILS_FONT))
                    .text_center()
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("exportDialog.disk_details")))
                    .child(project_name),
            )
            .child(
                div()
                    .id("save-card-button")
                    .test_support()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(d::CARD_BUTTON_H))
                    .mt(px(d::CARD_BUTTON_TOP))
                    .mb(px(d::CARD_BUTTON_BOTTOM))
                    .px(px(d::CARD_BUTTON_PAD))
                    .rounded(px(d::CARD_BUTTON_RADIUS))
                    .bg(rgb(CARD_LIME))
                    .hover(move |s| s.bg(rgb(CARD_LIME_DARKER)))
                    .text_size(px(d::CARD_BUTTON_FONT))
                    .text_color(rgb(0xffffff))
                    .child(SharedString::from(t("exportDialog.disk_button")))
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.pending_scene_save = true;
                    })),
            );

        let content = div()
            .flex()
            .flex_col()
            .w(px(d::WIDTH))
            .max_w_full()
            .p(px(d::PADDING))
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .child(
                div()
                    .pb(px(d::TITLE_PAD_BOTTOM))
                    .text_size(px(d::TITLE_FONT))
                    .text_color(tokens.surface_text())
                    .child(SharedString::from(t("menu.save"))),
            )
            .child(div().h(px(1.0)).w_full().bg(tokens.dialog_border()))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .w_full()
                    .mt(px(d::TITLE_GAP))
                    .child(card),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("save-dialog-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| this.show_save_dialog = false)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(d::MODAL_PADDING))
                    .child(content),
            )
            .into_any_element()
    }

    const EXPORT_SCALES: [f32; 3] = [1.0, 2.0, 3.0];

    fn render_image_dialog(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let tokens = crate::design::Tokens::for_palette(palette_of(self.document.theme));
        let t = |k: &str| self.i18n.t(k);

        use crate::design::size::export_dialog as d;

        // The filename field reuses the save dialog's caret and selection
        // state, so both modals behave identically while typing.
        let name = self.project_name.as_str();
        let name_len = name.len();
        let caret = self.name_caret.min(name_len);
        let (sel_start, sel_end) = self.save_name_range();
        let caret_color = tokens.surface_text();
        let caret_element = || {
            div()
                .flex_shrink_0()
                .w(px(1.0))
                .h(px(d::NAME_FIELD_FONT))
                .bg(caret_color)
                .with_animation(
                    "export-name-caret",
                    Animation::new(std::time::Duration::from_millis(1000)).repeat_synced(),
                    |el, progress| el.opacity(if progress < 0.5 { 1.0 } else { 0.0 }),
                )
                .into_any_element()
        };

        let mut cuts = [0usize, name_len, caret, sel_start, sel_end];
        cuts.sort_unstable();
        let mut runs: Vec<gpui_kit::AnyElement> = Vec::new();
        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start >= end {
                continue;
            }
            if start == caret {
                runs.push(caret_element());
            }
            let mut run = div().child(SharedString::from(name[start..end].to_string()));
            if start < sel_end && sel_start < end {
                run = run
                    .bg(tokens.selected_bg())
                    .text_color(tokens.selected_fg());
            }
            runs.push(run.into_any_element());
        }
        if caret == name_len {
            runs.push(caret_element());
        }

        let name_field = div()
            .id("export-filename")
            .test_support()
            .aria_label(SharedString::from(t("labels.fileTitle")))
            .aria_value(SharedString::from(self.project_name_value()))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .mt(px(d::NAME_FIELD_TOP))
            .w(px(d::NAME_FIELD_W))
            .h(px(d::NAME_FIELD_H))
            .px(px(d::NAME_FIELD_PAD))
            .border(px(1.0))
            .border_color(tokens.input_border())
            .hover(move |s| s.border_color(tokens.brand_hover()))
            .rounded(px(d::NAME_FIELD_RADIUS))
            .bg(tokens.surface_lowest())
            .text_size(px(d::NAME_FIELD_FONT))
            .text_center()
            .text_color(tokens.surface_text())
            .children(runs);

        // A checkerboard sits behind the preview so transparent exports read as
        // transparent rather than white, matching the upstream dialog.
        let checker = canvas(
            move |_bounds, _window, _cx| (),
            move |bounds, _state, window, _cx| {
                let cell = d::CHECKER as f64;
                let width = bounds.size.width.to_f64();
                let height = bounds.size.height.to_f64();
                let cols = (width / cell).ceil() as i32;
                let rows = (height / cell).ceil() as i32;
                for row in 0..rows {
                    for col in 0..cols {
                        let light = (row + col) % 2 == 0;
                        let color = if light {
                            rgba(0xffffffff)
                        } else {
                            rgba(0xf0f0f0ff)
                        };
                        let x = bounds.origin.x + px((col as f64 * cell) as f32);
                        let y = bounds.origin.y + px((row as f64 * cell) as f32);
                        let w = px(((width - col as f64 * cell).min(cell).max(0.0)) as f32);
                        let h = px(((height - row as f64 * cell).min(cell).max(0.0)) as f32);
                        if w <= px(0.0) || h <= px(0.0) {
                            continue;
                        }
                        window.paint_quad(quad(
                            gpui_kit::Bounds {
                                origin: gpui_kit::point(x, y),
                                size: gpui_kit::size(w, h),
                            },
                            Corners::default(),
                            color,
                            Edges::default(),
                            transparent_black(),
                            BorderStyle::default(),
                        ));
                    }
                }
            },
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full();

        let preview_image = match &self.export_preview {
            Some(image) => img(image.clone())
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .text_size(px(13.0))
                .text_color(tokens.gray_60())
                .child(SharedString::from(t("canvasError.cannotShowPreview")))
                .into_any_element(),
        };

        let preview = div()
            .id("export-preview")
            .test_support()
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(d::PREVIEW_H))
            .rounded(px(d::PREVIEW_RADIUS))
            .border(px(d::PREVIEW_BORDER))
            .border_color(if tokens.palette.is_dark() {
                rgb(0x5c5c5c)
            } else {
                rgb(0xd6d6d6)
            })
            .overflow_hidden()
            .child(checker)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(d::PREVIEW_PAD))
                    .child(preview_image),
            );

        let switch = |on: bool| -> gpui_kit::AnyElement {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(d::SWITCH_W))
                .h(px(d::SWITCH_H))
                .p(px(2.0))
                .rounded_full()
                .bg(if on {
                    tokens.accent()
                } else {
                    tokens.surface_low()
                })
                .border_1()
                .border_color(if on {
                    tokens.accent()
                } else {
                    tokens.input_border()
                })
                .when(on, |s| s.justify_end())
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(if on {
                            d::SWITCH_THUMB_ON
                        } else {
                            d::SWITCH_THUMB
                        }))
                        .rounded_full()
                        .bg(if on {
                            rgb(0xffffff)
                        } else {
                            tokens.surface_text()
                        }),
                )
                .into_any_element()
        };

        let setting_row = |id: &'static str,
                           label: String,
                           help: Option<String>,
                           control: gpui_kit::AnyElement,
                           handler: Handler<ClickEvent>|
         -> gpui_kit::AnyElement {
            let mut label_row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(d::SETTING_ICON_GAP))
                .text_size(px(d::SETTING_LABEL_FONT))
                .text_color(tokens.surface_text())
                .child(SharedString::from(label));
            if let Some(tip) = help {
                label_row = label_row.child(
                    div()
                        .id("export-embed-help")
                        .aria_label(SharedString::from(tip))
                        .child(crate::icons::icon(
                            "HelpIcon",
                            d::SETTING_ICON,
                            tokens.gray_60(),
                        )),
                );
            }
            div()
                .id(id)
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(d::SETTINGS_GAP))
                .w_full()
                .cursor_pointer()
                .child(label_row)
                .child(control)
                .on_click(handler)
                .into_any_element()
        };

        let action_button = |id: &'static str,
                             icon_name: &'static str,
                             label: String,
                             handler: Handler<ClickEvent>|
         -> gpui_kit::AnyElement {
            div()
                .id(id)
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .h(px(d::BUTTON_H))
                .px(px(d::BUTTON_PAD_X))
                .rounded(px(d::BUTTON_RADIUS))
                .bg(tokens.accent())
                .hover(move |s| s.bg(tokens.brand_hover()))
                .text_size(px(d::BUTTON_FONT))
                .text_color(rgb(0xffffff))
                .cursor_pointer()
                .child(crate::icons::icon(icon_name, d::BUTTON_ICON, rgb(0xffffff)))
                .child(SharedString::from(label))
                .on_click(handler)
                .into_any_element()
        };

        let mut settings: Vec<gpui_kit::AnyElement> = Vec::new();
        if !self.document.selected.is_empty() {
            settings.push(setting_row(
                "export-row-only-selected",
                t("imageExportDialog.label.onlySelected"),
                None,
                switch(self.export_only_selected),
                Self::act(cx, |this, _, _, _| {
                    this.export_only_selected = !this.export_only_selected;
                    this.refresh_export_preview();
                }),
            ));
        }
        settings.push(setting_row(
            "export-row-background",
            t("imageExportDialog.label.withBackground"),
            None,
            switch(self.export_with_background),
            Self::act(cx, |this, _, _, _| {
                this.export_with_background = !this.export_with_background;
                this.refresh_export_preview();
            }),
        ));
        settings.push(setting_row(
            "export-row-dark-mode",
            t("imageExportDialog.label.darkMode"),
            None,
            switch(self.export_dark_mode),
            Self::act(cx, |this, _, _, _| {
                this.export_dark_mode = !this.export_dark_mode;
                this.refresh_export_preview();
            }),
        ));
        settings.push(setting_row(
            "export-row-embed-scene",
            t("imageExportDialog.label.embedScene"),
            Some(t("imageExportDialog.tooltip.embedScene")),
            switch(self.export_embed_scene),
            Self::act(cx, |this, _, _, _| {
                this.export_embed_scene = !this.export_embed_scene;
            }),
        ));

        let current_scale = self.export_scale;
        let radio = div()
            .flex()
            .flex_row()
            .p(px(d::RADIO_PAD))
            .rounded(px(d::RADIO_RADIUS))
            .bg(tokens.surface_low())
            .children(Self::EXPORT_SCALES.iter().map(|scale| {
                let value = *scale;
                let selected = (current_scale - value).abs() < f32::EPSILON;
                div()
                    .id(SharedString::from(format!("export-scale-{}", value as i32)))
                    .test_support()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(d::RADIO_ITEM_H))
                    .min_w(px(d::RADIO_ITEM_MIN_W))
                    .px(px(8.0))
                    .rounded(px(d::RADIO_ITEM_RADIUS))
                    .text_size(px(d::RADIO_FONT))
                    .cursor_pointer()
                    .when(selected, |s| {
                        s.bg(tokens.accent()).text_color(rgb(0xffffff))
                    })
                    .when(!selected, |s| s.text_color(tokens.surface_text()))
                    .child(SharedString::from(format!("{}×", value as i32)))
                    .on_click(Self::act(cx, move |this, _, _, _| {
                        this.export_scale = value;
                    }))
            }));
        settings.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(d::SETTINGS_GAP))
                .w_full()
                .child(
                    div()
                        .text_size(px(d::SETTING_LABEL_FONT))
                        .text_color(tokens.surface_text())
                        .child(SharedString::from(t("imageExportDialog.label.scale"))),
                )
                .child(radio)
                .into_any_element(),
        );

        let actions = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(d::BUTTON_GAP))
            .mt(px(d::BUTTON_ROW_TOP))
            .child(action_button(
                "export-btn-png",
                "downloadIcon",
                t("imageExportDialog.button.exportToPng"),
                Self::act(cx, |this, _, _, cx| this.export_png_picker(cx)),
            ))
            .child(action_button(
                "export-btn-svg",
                "downloadIcon",
                t("imageExportDialog.button.exportToSvg"),
                Self::act(cx, |this, _, _, cx| this.export_svg_picker(cx)),
            ))
            .child(action_button(
                "export-btn-copy",
                "copyIcon",
                t("imageExportDialog.button.copyPngToClipboard"),
                Self::act(cx, |this, _, _, cx| {
                    this.export_dialog_copy_png(cx);
                }),
            ));

        let content = div()
            .flex()
            .flex_col()
            .w(px(d::WIDTH))
            .max_w_full()
            .p(px(d::CONTENT_PADDING))
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(d::PREVIEW_GAP))
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .w(gpui_kit::relative(0.55))
                            .child(preview)
                            .child(name_field),
                    )
                    .child(
                        div()
                            .id("export-settings")
                            .test_support()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(d::HEADER_FONT))
                                    .font_bold()
                                    .text_color(tokens.surface_text())
                                    .child(SharedString::from(t("imageExportDialog.header"))),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(d::SETTINGS_GAP))
                                    .mt(px(d::SETTINGS_GAP))
                                    .w_full()
                                    .children(settings),
                            )
                            .child(div().flex_1())
                            .child(actions),
                    ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("image-dialog-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| this.close_image_dialog())),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(d::MODAL_PADDING))
                    .child(content),
            )
            .into_any_element()
    }

    /// Mirrors upstream's `TTDDialog` with the Mermaid tab active: description
    /// with underlined diagram links, a gutter-numbered source editor on the
    /// left and the live rendered preview on the right.
    fn render_mermaid_dialog(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        use crate::design::size::ttd_dialog as d;

        let tokens = Tokens::for_palette(palette_of(self.document.theme));
        let t = |key: &str| self.i18n.t(key);
        let ttd = d::MODAL_PADDING;
        let dialog_w = (self.viewport.0 as f32 - ttd * 2.0).clamp(560.0, d::WIDTH);
        let dialog_h = (self.viewport.1 as f32 - ttd * 2.0).clamp(d::MIN_HEIGHT, d::MAX_HEIGHT);

        let tab_button = |tab: MermaidTab,
                          id: &'static str,
                          label: String,
                          badge: Option<String>|
         -> gpui_kit::AnyElement {
            let active = self.mermaid_tab == tab;
            div()
                .id(id)
                .test_support()
                .flex()
                .flex_row()
                .items_center()
                .h(px(d::TAB_H))
                .px(px(d::TAB_PAD_X))
                .text_size(px(d::TAB_FONT))
                .font_weight(FontWeight(600.0))
                .text_color(tokens.surface_text())
                .cursor_pointer()
                .when(active, |s| {
                    s.border_b(px(d::TAB_BORDER)).border_color(tokens.accent())
                })
                .child(SharedString::from(label))
                .children(badge.map(|text| {
                    div()
                        .ml(px(d::BADGE_MARGIN_LEFT))
                        .px(px(d::BADGE_PAD_X))
                        .py(px(d::BADGE_PAD_Y))
                        .rounded(px(d::BADGE_RADIUS))
                        .bg(tokens.accent())
                        .text_size(px(d::BADGE_FONT))
                        .text_color(tokens.surface_lowest())
                        .child(SharedString::from(text))
                }))
                .on_click(Self::act(cx, move |this, _, _, _| {
                    this.mermaid_tab = tab;
                }))
                .into_any_element()
        };

        let tabs = div()
            .flex()
            .flex_row()
            .w_full()
            .mb(px(16.0))
            .px(px(d::TABS_PAD_X))
            .border_b(px(1.0))
            .border_color(tokens.surface_high())
            .child(tab_button(
                MermaidTab::TextToDiagram,
                "mermaid-tab-text",
                t("labels.textToDiagram"),
                Some(t("chat.aiBeta")),
            ))
            .child(tab_button(
                MermaidTab::Mermaid,
                "mermaid-tab-mermaid",
                t("mermaid.label"),
                None,
            ));

        let (description, links) = mermaid_description_runs(&t("mermaid.description"));
        let link_color = tokens.link();
        let mut description_highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
        let mut cursor = 0usize;
        for range in links {
            if cursor < range.start {
                description_highlights.push((
                    cursor..range.start,
                    HighlightStyle {
                        font_style: Some(FontStyle::Italic),
                        ..Default::default()
                    },
                ));
            }
            description_highlights.push((
                range.clone(),
                HighlightStyle {
                    color: Some(link_color.into()),
                    font_style: Some(FontStyle::Italic),
                    underline: Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(link_color.into()),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            ));
            cursor = range.end;
        }
        if cursor < description.len() {
            description_highlights.push((
                cursor..description.len(),
                HighlightStyle {
                    font_style: Some(FontStyle::Italic),
                    ..Default::default()
                },
            ));
        }
        let description = div()
            .w_full()
            .mt(px(d::DESC_TOP))
            .mb(px(d::DESC_BOTTOM))
            .text_size(px(d::DESC_FONT))
            .line_height(px(d::DESC_LINE_H))
            .italic()
            .font_weight(FontWeight(500.0))
            .text_color(tokens.surface_text())
            .child(StyledText::new(description).with_highlights(description_highlights));

        let body = match self.mermaid_tab {
            MermaidTab::TextToDiagram => div()
                .flex()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .p(px(d::CONTENT_PADDING))
                        .border_1()
                        .border_color(tokens.dialog_border())
                        .rounded(px(d::INPUT_RADIUS))
                        .text_size(px(d::DESC_FONT))
                        .text_color(tokens.hint_desc())
                        .child(SharedString::from(
                            t("chat.inputPlaceholder").replace("{{shortcut}}", "Enter"),
                        )),
                )
                .into_any_element(),
            MermaidTab::Mermaid => self.render_mermaid_panels(tokens, &t, cx),
        };

        let content = div()
            .flex()
            .flex_col()
            .w(px(dialog_w))
            .h(px(dialog_h))
            .max_w_full()
            .bg(tokens.island())
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::RADIUS))
            .shadow(crate::design::modal_shadow())
            .occlude()
            .overflow_hidden()
            .child(tabs)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .px(px(d::PANEL_PAD_X))
                    .pt(px(d::CONTENT_TOP_PADDING))
                    .pb(px(d::CONTENT_BOTTOM_PADDING))
                    .child(description)
                    .child(body),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .child(
                div()
                    .id("mermaid-dialog-backdrop")
                    .test_support()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .occlude()
                    .bg(tokens.overlay_backdrop())
                    .on_click(Self::act(cx, |this, _, _, _| this.close_mermaid_dialog())),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(ttd))
                    .child(content),
            )
            .into_any_element()
    }

    fn render_mermaid_panels(
        &self,
        tokens: Tokens,
        t: &impl Fn(&str) -> String,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        use crate::design::size::ttd_dialog as d;

        let editor = self.render_mermaid_editor(tokens, cx);
        let preview = self.render_mermaid_preview(tokens, t);

        let can_insert = !self.mermaid_preview.is_empty();
        // Upstream uses `#fff` on the primary button in light mode and
        // `--color-gray-100` in the dark theme.
        let button_fg = if tokens.palette.is_dark() {
            tokens.surface_text()
        } else {
            rgb(0xffffff)
        };
        let insert_button = div()
            .id("mermaid-insert")
            .test_support()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .h(px(d::BUTTON_H))
            .min_w(px(d::BUTTON_MIN_W))
            .px(px(d::BUTTON_PAD_X))
            .rounded(px(d::BUTTON_RADIUS))
            .bg(tokens.accent())
            .text_size(px(d::BUTTON_FONT))
            .font_weight(FontWeight(600.0))
            .text_color(button_fg)
            .when(!can_insert, |s| s.opacity(0.5))
            .when(can_insert, |s| {
                s.cursor_pointer()
                    .hover(|s| s.bg(tokens.brand_hover()))
                    .on_click(Self::act(cx, |this, _, _, _| {
                        this.insert_mermaid_diagram();
                    }))
            })
            .child(SharedString::from(t("mermaid.button")))
            .child(crate::icons::icon("ArrowIcon", d::BUTTON_FONT, button_fg))
            .into_any_element();

        let shortcut = |label: &str| -> gpui_kit::AnyElement {
            div()
                .px(px(d::SHORTCUT_PAD_X))
                .py(px(d::SHORTCUT_PAD_Y))
                .rounded(px(d::SHORTCUT_RADIUS))
                .border_1()
                .border_color(rgb(0x808080))
                .text_size(px(d::SHORTCUT_FONT))
                .font_family("monospace")
                .text_color(tokens.surface_text())
                .child(SharedString::from(label.to_string()))
                .into_any_element()
        };
        let shortcut_hint = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(d::SHORTCUT_GAP))
            .ml(px(d::SHORTCUT_MARGIN))
            .opacity(0.6)
            .child(shortcut(if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Ctrl"
            }))
            .child(shortcut(&t("keys.enter")));

        // Upstream reserves the same action-row height in the left panel (with
        // `visibility: hidden`), keeping both panels exactly the same height.
        let action_row = |button: gpui_kit::AnyElement,
                          shortcut: gpui_kit::AnyElement,
                          hidden: bool|
         -> gpui_kit::AnyElement {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_start()
                .flex_shrink_0()
                .w_full()
                .h(px(d::BUTTON_H + d::BUTTON_TOP + d::BUTTON_BOTTOM))
                .pt(px(d::BUTTON_TOP))
                .pb(px(d::BUTTON_BOTTOM))
                .when(hidden, |s| s.opacity(0.0))
                .child(button)
                .child(shortcut)
                .into_any_element()
        };

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .gap(px(d::PANEL_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .child(editor)
                    .child(action_row(
                        div().into_any_element(),
                        div().into_any_element(),
                        true,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .child(preview)
                    .child(action_row(
                        insert_button,
                        shortcut_hint.into_any_element(),
                        false,
                    )),
            )
            .into_any_element()
    }

    fn render_mermaid_editor(
        &self,
        tokens: Tokens,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        use crate::design::size::ttd_dialog as d;

        let colors = MermaidCodeColors::for_dark(tokens.palette.is_dark());
        let base = colors.text;
        let selected_bg = tokens.selected_bg();
        let caret_color = colors.caret;
        let (selection_start, selection_end) = self.mermaid_range();
        let has_selection = selection_start < selection_end;
        let caret = self.mermaid_caret.min(self.mermaid_text.len());

        let source = self.mermaid_text.as_str();
        let mut rows = Vec::new();
        let mut line_start = 0;
        let mut line_index = 0usize;
        loop {
            let line_end = mermaid_line_end(source, line_start);
            let line = &source[line_start..line_end];
            let caret_in_line = caret >= line_start && caret <= line_end;
            let caret_column = caret.saturating_sub(line_start).min(line.len());

            let token_runs = mermaid_token_runs(line, &colors);
            let mut cuts = vec![0usize, line.len()];
            for (range, _) in &token_runs {
                cuts.push(range.start);
                cuts.push(range.end);
            }
            if has_selection {
                let start = selection_start.max(line_start).min(line_end) - line_start;
                let end = selection_end.max(line_start).min(line_end) - line_start;
                cuts.push(start);
                cuts.push(end);
            }
            if caret_in_line {
                cuts.push(caret_column);
            }
            cuts.sort_unstable();
            cuts.dedup();

            let mut segments = Vec::new();
            for pair in cuts.windows(2) {
                let (start, end) = (pair[0], pair[1]);
                if start >= end {
                    continue;
                }
                let color = token_runs
                    .iter()
                    .find(|(range, _)| range.start <= start && end <= range.end)
                    .map(|(_, color)| *color);
                let selected = has_selection
                    && start + line_start >= selection_start
                    && end + line_start <= selection_end;
                segments.push((start, end, color, selected));
            }

            let highlights = |from: usize, to: usize| {
                let mut highlights = Vec::new();
                for &(start, end, color, selected) in &segments {
                    if end <= from || start >= to {
                        continue;
                    }
                    let start = start.max(from) - from;
                    let end = end.min(to) - from;
                    if start >= end {
                        continue;
                    }
                    highlights.push((
                        start..end,
                        HighlightStyle {
                            color: color.map(Into::into),
                            background_color: selected.then(|| selected_bg.into()),
                            ..Default::default()
                        },
                    ));
                }
                highlights
            };

            let mut row = div()
                .id(SharedString::from(format!("mermaid-line-{line_index}")))
                .flex()
                .flex_row()
                .items_center()
                .min_h(px(d::INPUT_LINE_H))
                .w_full()
                .text_size(px(d::INPUT_FONT))
                .line_height(px(d::INPUT_LINE_H))
                .font_family(MERMAID_CODE_FONT)
                .text_color(base)
                .on_click(Self::act(cx, move |this, _, _, _| {
                    let len = this.mermaid_text.len();
                    let start = mermaid_line_start(&this.mermaid_text, len.min(line_start));
                    let end = mermaid_line_end(&this.mermaid_text, start);
                    this.mermaid_caret = end;
                    this.mermaid_anchor = end;
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .w(px(d::GUTTER_W))
                        .h(px(d::INPUT_LINE_H))
                        .pr(px(d::GUTTER_GAP))
                        .text_size(px(d::INPUT_FONT))
                        .text_color(colors.gutter)
                        .child(SharedString::from(format!("{}", line_index + 1))),
                );

            if caret_in_line {
                let (prefix, suffix) = line.split_at(caret_column);
                if !prefix.is_empty() {
                    row = row.child(
                        StyledText::new(prefix).with_highlights(highlights(0, caret_column)),
                    );
                }
                row = row.child(
                    div()
                        .flex_shrink_0()
                        .w(px(2.0))
                        .h(px(d::INPUT_LINE_H))
                        .bg(caret_color),
                );
                if !suffix.is_empty() {
                    row = row.child(
                        StyledText::new(suffix)
                            .with_highlights(highlights(caret_column, line.len())),
                    );
                }
            } else if !line.is_empty() {
                row = row.child(StyledText::new(line).with_highlights(highlights(0, line.len())));
            }
            rows.push(row);

            if line_end >= source.len() {
                break;
            }
            line_start = line_end + 1;
            line_index += 1;
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .border_1()
            .border_color(tokens.dialog_border())
            .rounded(px(d::INPUT_RADIUS))
            .bg(colors.background)
            .overflow_hidden()
            .child(
                div()
                    .id("mermaid-input")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .py(px(d::INPUT_PAD))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_mermaid_preview(
        &self,
        tokens: Tokens,
        t: &impl Fn(&str) -> String,
    ) -> gpui_kit::AnyElement {
        use crate::design::size::ttd_dialog as d;

        let dark = tokens.palette.is_dark();
        let fit_bounds = {
            let refs: Vec<&Element> = self.mermaid_preview.iter().collect();
            crate::core::bounds::common_bounds(&refs)
                .map(|b| (b.min_x, b.min_y, b.max_x - b.min_x, b.max_y - b.min_y))
        };
        let mut elements = self.mermaid_preview.clone();
        if dark {
            darken_element_colors(&mut elements);
        }
        let images = self.image_sources.clone();
        let background = if dark { rgb(0x121212) } else { rgb(0xffffff) };
        let paint = canvas(
            move |_bounds, _window, _cx| (),
            move |bounds, _state, window, cx| {
                window.paint_quad(quad(
                    bounds,
                    Corners::default(),
                    background,
                    Edges::default(),
                    transparent_black(),
                    BorderStyle::default(),
                ));
                let Some((min_x, min_y, width, height)) = fit_bounds else {
                    return;
                };
                if width <= 0.0 || height <= 0.0 {
                    return;
                }
                let available_w = bounds.size.width.to_f64();
                let available_h = bounds.size.height.to_f64();
                let pad = d::PREVIEW_PAD as f64;
                let zoom = ((available_w - pad * 2.0) / width)
                    .min((available_h - pad * 2.0) / height)
                    .max(0.01);
                let scroll_x =
                    bounds.origin.x.to_f64() + (available_w - width * zoom) / 2.0 - min_x * zoom;
                let scroll_y =
                    bounds.origin.y.to_f64() + (available_h - height * zoom) / 2.0 - min_y * zoom;
                crate::paint_elements(&elements, zoom, scroll_x, scroll_y, &images, window, cx);
            },
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full();

        let overlay = if let Some(message) = &self.mermaid_error {
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .p(px(24.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_bold()
                        .text_color(tokens.danger())
                        .child(SharedString::from(t("chat.errors.mermaidParseError"))),
                )
                .child(
                    div()
                        .w_full()
                        .text_size(px(12.0))
                        .font_family("monospace")
                        .text_color(tokens.gray_60())
                        .child(SharedString::from(message.clone())),
                )
                .into_any_element()
        } else if self.mermaid_text.trim().is_empty() {
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.0))
                .text_size(px(13.0))
                .text_color(tokens.gray_60())
                .child(SharedString::from(t("mermaid.inputPlaceholder")))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        div()
            .id("mermaid-preview")
            .test_support()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .rounded(px(d::PREVIEW_RADIUS))
            .border(px(d::PREVIEW_BORDER))
            .border_color(if tokens.palette.is_dark() {
                rgb(0x5c5c5c)
            } else {
                rgb(0xd6d6d6)
            })
            .bg(background)
            .overflow_hidden()
            .child(paint)
            .child(overlay)
            .into_any_element()
    }
}

#[cfg(test)]
mod toolbar_tests {
    use super::extra_tools_trigger;
    use crate::ui::Tool;

    #[test]
    fn extra_tools_trigger_mirrors_the_active_extra_tool() {
        assert_eq!(extra_tools_trigger(Tool::Image), ("ImageIcon", true));
        assert_eq!(extra_tools_trigger(Tool::Frame), ("frameToolIcon", true));
        assert_eq!(
            extra_tools_trigger(Tool::Laser),
            ("laserPointerToolIcon", true)
        );
        assert_eq!(extra_tools_trigger(Tool::Embeddable), ("EmbedIcon", true));
        assert_eq!(extra_tools_trigger(Tool::Selection), ("DotsIcon", false));
        assert_eq!(extra_tools_trigger(Tool::Rectangle), ("DotsIcon", false));
    }
}
