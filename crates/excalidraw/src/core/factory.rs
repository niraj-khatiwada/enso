use crate::{arrowhead::Arrowhead, element::*, geometry::Point, types::*};

pub struct ElementOptions {
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roughness: f64,
    pub opacity: f64,
}

impl Default for ElementOptions {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".to_string(),
            background_color: "transparent".to_string(),
            fill_style: FillStyle::Hachure,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roughness: 1.0,
            opacity: 100.0,
        }
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn apply_options(base: &mut ElementBase, options: &ElementOptions) {
    base.stroke_color = options.stroke_color.clone();
    base.background_color = options.background_color.clone();
    base.fill_style = options.fill_style;
    base.stroke_width = options.stroke_width;
    base.stroke_style = options.stroke_style;
    base.roughness = options.roughness;
    base.opacity = options.opacity;
}

pub fn new_rectangle(x: f64, y: f64, w: f64, h: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    Element::Rectangle { base }
}

pub fn new_diamond(x: f64, y: f64, w: f64, h: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Diamond);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    Element::Diamond { base }
}

pub fn new_ellipse(x: f64, y: f64, w: f64, h: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Ellipse);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    Element::Ellipse { base }
}

pub fn new_line(points: Vec<Point>, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Line);
    let bounds = crate::bounds::Bounds::from_points(&points);
    base.x = bounds.min_x;
    base.y = bounds.min_y;
    base.width = bounds.width();
    base.height = bounds.height();
    apply_options(&mut base, options);
    Element::Line(LineElement {
        linear: LinearElement {
            base,
            points,
            start_binding: None,
            end_binding: None,
            start_arrowhead: None,
            end_arrowhead: None,
        },
        polygon: false,
    })
}

pub fn new_arrow(
    points: Vec<Point>,
    start_arrowhead: Option<Arrowhead>,
    end_arrowhead: Option<Arrowhead>,
    options: &ElementOptions,
) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Arrow);
    let bounds = crate::bounds::Bounds::from_points(&points);
    base.x = bounds.min_x;
    base.y = bounds.min_y;
    base.width = bounds.width();
    base.height = bounds.height();
    apply_options(&mut base, options);
    Element::Arrow(ArrowElement {
        linear: LinearElement {
            base,
            points,
            start_binding: None,
            end_binding: None,
            start_arrowhead,
            end_arrowhead,
        },
        elbowed: false,
        fixed_segments: None,
        start_is_special: None,
        end_is_special: None,
    })
}

pub fn new_freedraw(points: Vec<Point>, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Freedraw);
    let bounds = crate::bounds::Bounds::from_points(&points);
    base.x = bounds.min_x;
    base.y = bounds.min_y;
    base.width = bounds.width();
    base.height = bounds.height();
    apply_options(&mut base, options);
    let pressures = vec![0.5; points.len()];
    Element::Freedraw(FreeDrawElement {
        base,
        points,
        pressures,
        simulate_pressure: true,
        stroke_options: StrokeOptions::default(),
    })
}

pub fn new_text(x: f64, y: f64, text: &str, font_size: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Text);
    base.x = x;
    base.y = y;
    base.width = font_size * text.chars().count() as f64 * 0.6;
    base.height = font_size * 1.4;
    apply_options(&mut base, options);
    Element::Text(TextElement {
        base,
        font_size,
        font_family: FontFamily::Excalifont,
        base_font_size: None,
        text: text.to_string(),
        text_align: TextAlign::Left,
        vertical_align: VerticalAlign::Top,
        container_id: None,
        original_text: text.to_string(),
        auto_resize: true,
        line_height: 1.25,
        label_position: None,
    })
}

pub fn new_frame(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    name: Option<String>,
    options: &ElementOptions,
) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Frame);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    Element::Frame(FrameElement { base, name })
}

pub fn new_embeddable(x: f64, y: f64, w: f64, h: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Embeddable);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    // Upstream defaults `currentItemRoundness` to "round" and switches
    // embeddables to the adaptive radius, so a fresh frame has rounded corners.
    base.roundness = Some(Roundness {
        kind: RoundnessType::AdaptiveRadius,
        value: None,
    });
    Element::Embeddable(EmbeddableElement { base })
}

pub fn new_image(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    file_id: Option<String>,
    options: &ElementOptions,
) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::Image);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);
    Element::Image(ImageElement {
        base,
        file_id,
        status: ImageStatus::Pending,
        scale: [1.0, 1.0],
        crop: None,
    })
}

pub fn new_sticky_note(x: f64, y: f64, w: f64, h: f64, options: &ElementOptions) -> Element {
    let mut base = ElementBase::new(new_id(), ElementType::StickyNote);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    apply_options(&mut base, options);

    base.roundness = Some(Roundness {
        kind: RoundnessType::ProportionalRadius,
        value: Some(0.1),
    });
    if crate::color::is_transparent(&base.background_color) {
        base.background_color = "#ffec99".to_string();
        base.fill_style = FillStyle::Solid;
    }
    Element::StickyNote(StickyNoteElement {
        base,
        base_height: h,
    })
}

pub fn new_sticky_note_with_text(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    options: &ElementOptions,
) -> (Element, Element) {
    let mut note = new_sticky_note(x, y, w, h, options);
    let text_opts = ElementOptions {
        stroke_color: "#1e1e1e".to_string(),
        background_color: "transparent".to_string(),
        fill_style: FillStyle::Solid,
        stroke_width: options.stroke_width,
        stroke_style: StrokeStyle::Solid,
        roughness: 0.0,
        opacity: 100.0,
    };
    let font_size = 20.0;
    let mut text = new_text(x + 8.0, y + 8.0, "", font_size, &text_opts);
    let note_id = note.id().to_string();
    let text_id = text.id().to_string();
    if let Element::Text(t) = &mut text {
        t.container_id = Some(note_id.clone());
    }
    note.base_mut().bound_elements = Some(vec![BoundElement {
        id: text_id,
        kind: BoundElementType::Text,
    }]);
    (note, text)
}

pub fn streamline_points(points: &[Point], factor: f64) -> Vec<Point> {
    if points.len() < 3 || factor <= 0.0 {
        return points.to_vec();
    }
    let f = factor.clamp(0.0, 0.95);
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    let mut prev = points[0];
    for p in points.iter().skip(1) {
        let next = Point::new(
            prev.x + (p.x - prev.x) * (1.0 - f),
            prev.y + (p.y - prev.y) * (1.0 - f),
        );
        out.push(next);
        prev = next;
    }
    *out.last_mut().unwrap() = *points.last().unwrap();
    out
}
