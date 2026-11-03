use serde::{Deserialize, Serialize};

use crate::{arrowhead::Arrowhead, geometry::Point, types::*};

fn default_line_height() -> f64 {
    1.25
}

fn unit_scale() -> [f64; 2] {
    [1.0, 1.0]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementType {
    Selection,
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Text,
    Image,
    Frame,
    MagicFrame,
    Iframe,
    Embeddable,
    StickyNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LinearElementSubType {
    #[serde(rename = "line")]
    Line,
    #[serde(rename = "sharpArrow")]
    SharpArrow,
    #[serde(rename = "curvedArrow")]
    CurvedArrow,
    #[serde(rename = "elbowArrow")]
    ElbowArrow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundElement {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: BoundElementType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundElementType {
    Arrow,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub natural_width: f64,
    pub natural_height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageStatus {
    #[default]
    Pending,
    Saved,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindMode {
    Inside,
    Orbit,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedPointBinding {
    pub element_id: String,
    #[serde(default = "centered_fixed_point")]
    pub fixed_point: [f64; 2],
    #[serde(default = "default_bind_mode")]
    pub mode: BindMode,
}

fn centered_fixed_point() -> [f64; 2] {
    [0.5001, 0.5001]
}

fn default_bind_mode() -> BindMode {
    BindMode::Orbit
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FixedSegment {
    pub start: Point,
    pub end: Point,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementBase {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub fill_style: FillStyle,
    pub stroke_width: f64,
    pub stroke_style: StrokeStyle,
    pub roundness: Option<Roundness>,
    pub roughness: f64,
    pub opacity: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub seed: i64,
    pub version: i64,
    pub version_nonce: i64,
    pub index: Option<String>,
    pub is_deleted: bool,
    pub group_ids: Vec<String>,
    pub frame_id: Option<String>,
    pub bound_elements: Option<Vec<BoundElement>>,
    pub updated: i64,
    pub created: Option<i64>,
    pub link: Option<String>,
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<serde_json::Value>,
}

impl ElementBase {
    pub fn new(id: String, _kind: ElementType) -> Self {
        Self {
            id,
            x: 0.0,
            y: 0.0,
            stroke_color: "#1e1e1e".to_string(),
            background_color: "transparent".to_string(),
            fill_style: FillStyle::Hachure,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roundness: None,
            roughness: 1.0,
            opacity: 100.0,
            width: 0.0,
            height: 0.0,
            angle: 0.0,
            seed: 1,
            version: 1,
            version_nonce: 0,
            index: None,
            is_deleted: false,
            group_ids: Vec::new(),
            frame_id: None,
            bound_elements: None,
            updated: 0,
            created: None,
            link: None,
            locked: false,
            custom_data: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextElement {
    #[serde(flatten)]
    pub base: ElementBase,
    pub font_size: f64,
    pub font_family: FontFamily,
    pub base_font_size: Option<f64>,
    pub text: String,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub container_id: Option<String>,
    pub original_text: String,
    pub auto_resize: bool,
    #[serde(default = "default_line_height")]
    pub line_height: f64,
    pub label_position: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearElement {
    #[serde(flatten)]
    pub base: ElementBase,
    pub points: Vec<Point>,
    pub start_binding: Option<FixedPointBinding>,
    pub end_binding: Option<FixedPointBinding>,
    pub start_arrowhead: Option<Arrowhead>,
    pub end_arrowhead: Option<Arrowhead>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineElement {
    #[serde(flatten)]
    pub linear: LinearElement,
    #[serde(default)]
    pub polygon: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowElement {
    #[serde(flatten)]
    pub linear: LinearElement,
    #[serde(default)]
    pub elbowed: bool,
    pub fixed_segments: Option<Vec<FixedSegment>>,
    pub start_is_special: Option<bool>,
    pub end_is_special: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeDrawElement {
    #[serde(flatten)]
    pub base: ElementBase,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub pressures: Vec<f64>,
    #[serde(default)]
    pub simulate_pressure: bool,
    #[serde(default)]
    pub stroke_options: StrokeOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageElement {
    #[serde(flatten)]
    pub base: ElementBase,
    pub file_id: Option<String>,
    #[serde(default)]
    pub status: ImageStatus,
    #[serde(default = "unit_scale")]
    pub scale: [f64; 2],
    pub crop: Option<ImageCrop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameElement {
    #[serde(flatten)]
    pub base: ElementBase,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickyNoteElement {
    #[serde(flatten)]
    pub base: ElementBase,
    pub base_height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddableElement {
    #[serde(flatten)]
    pub base: ElementBase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IframeElement {
    #[serde(flatten)]
    pub base: ElementBase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Selection {
        #[serde(flatten)]
        base: ElementBase,
    },
    Rectangle {
        #[serde(flatten)]
        base: ElementBase,
    },
    Diamond {
        #[serde(flatten)]
        base: ElementBase,
    },
    Ellipse {
        #[serde(flatten)]
        base: ElementBase,
    },
    Line(LineElement),
    Arrow(ArrowElement),
    Freedraw(FreeDrawElement),
    Text(TextElement),
    Image(ImageElement),
    Frame(FrameElement),
    #[serde(rename = "magicframe")]
    MagicFrame(FrameElement),
    Iframe(IframeElement),
    Embeddable(EmbeddableElement),
    #[serde(rename = "stickynote")]
    StickyNote(StickyNoteElement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConvertTarget {
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    SharpArrow,
    CurvedArrow,
    ElbowArrow,
}

pub const GENERIC_TARGETS: [ConvertTarget; 3] = [
    ConvertTarget::Rectangle,
    ConvertTarget::Diamond,
    ConvertTarget::Ellipse,
];

pub const LINEAR_TARGETS: [ConvertTarget; 4] = [
    ConvertTarget::Line,
    ConvertTarget::SharpArrow,
    ConvertTarget::CurvedArrow,
    ConvertTarget::ElbowArrow,
];

impl ConvertTarget {
    pub fn is_linear(self) -> bool {
        matches!(
            self,
            Self::Line | Self::SharpArrow | Self::CurvedArrow | Self::ElbowArrow
        )
    }
}

fn bump(base: &mut ElementBase) {
    base.version += 1;
    base.version_nonce = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as i64;
}

static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Element {
    pub fn base(&self) -> &ElementBase {
        match self {
            Self::Selection { base }
            | Self::Rectangle { base }
            | Self::Diamond { base }
            | Self::Ellipse { base } => base,
            Self::Line(l) => &l.linear.base,
            Self::Arrow(a) => &a.linear.base,
            Self::Freedraw(f) => &f.base,
            Self::Text(t) => &t.base,
            Self::Image(i) => &i.base,
            Self::Frame(f) | Self::MagicFrame(f) => &f.base,
            Self::Iframe(i) => &i.base,
            Self::Embeddable(e) => &e.base,
            Self::StickyNote(s) => &s.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut ElementBase {
        match self {
            Self::Selection { base }
            | Self::Rectangle { base }
            | Self::Diamond { base }
            | Self::Ellipse { base } => base,
            Self::Line(l) => &mut l.linear.base,
            Self::Arrow(a) => &mut a.linear.base,
            Self::Freedraw(f) => &mut f.base,
            Self::Text(t) => &mut t.base,
            Self::Image(i) => &mut i.base,
            Self::Frame(f) | Self::MagicFrame(f) => &mut f.base,
            Self::Iframe(i) => &mut i.base,
            Self::Embeddable(e) => &mut e.base,
            Self::StickyNote(s) => &mut s.base,
        }
    }

    pub fn kind(&self) -> ElementType {
        match self {
            Self::Selection { .. } => ElementType::Selection,
            Self::Rectangle { .. } => ElementType::Rectangle,
            Self::Diamond { .. } => ElementType::Diamond,
            Self::Ellipse { .. } => ElementType::Ellipse,
            Self::Line(_) => ElementType::Line,
            Self::Arrow(_) => ElementType::Arrow,
            Self::Freedraw(_) => ElementType::Freedraw,
            Self::Text(_) => ElementType::Text,
            Self::Image(_) => ElementType::Image,
            Self::Frame(_) => ElementType::Frame,
            Self::MagicFrame(_) => ElementType::MagicFrame,
            Self::Iframe(_) => ElementType::Iframe,
            Self::Embeddable(_) => ElementType::Embeddable,
            Self::StickyNote(_) => ElementType::StickyNote,
        }
    }

    pub fn id(&self) -> &str {
        &self.base().id
    }

    pub fn is_deleted(&self) -> bool {
        self.base().is_deleted
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    pub fn is_linear(&self) -> bool {
        matches!(self, Self::Line(_) | Self::Arrow(_))
    }

    pub fn is_bindable(&self) -> bool {
        matches!(
            self,
            Self::Rectangle { .. }
                | Self::Diamond { .. }
                | Self::Ellipse { .. }
                | Self::Text(_)
                | Self::Image(_)
                | Self::Frame(_)
                | Self::MagicFrame(_)
                | Self::Iframe(_)
                | Self::Embeddable(_)
                | Self::StickyNote(_)
        )
    }

    pub fn is_convertible_linear(&self) -> bool {
        match self {
            Self::Line(_) => true,
            Self::Arrow(a) => a.linear.start_binding.is_none() && a.linear.end_binding.is_none(),
            _ => false,
        }
    }

    pub fn convertible_targets(&self) -> &'static [ConvertTarget] {
        match self {
            Self::Rectangle { .. } | Self::Diamond { .. } | Self::Ellipse { .. } => {
                &GENERIC_TARGETS
            }
            _ if self.is_convertible_linear() => &LINEAR_TARGETS,
            _ => &[],
        }
    }

    pub fn convert_target(&self) -> Option<ConvertTarget> {
        match self {
            Self::Rectangle { .. } => Some(ConvertTarget::Rectangle),
            Self::Diamond { .. } => Some(ConvertTarget::Diamond),
            Self::Ellipse { .. } => Some(ConvertTarget::Ellipse),
            Self::Line(_) => Some(ConvertTarget::Line),
            Self::Arrow(a) if a.elbowed => Some(ConvertTarget::ElbowArrow),
            Self::Arrow(a) if a.linear.base.roundness.is_some() => Some(ConvertTarget::CurvedArrow),
            Self::Arrow(_) => Some(ConvertTarget::SharpArrow),
            _ => None,
        }
    }

    pub fn convert_to(&self, target: ConvertTarget) -> Option<Element> {
        if !self.convertible_targets().contains(&target) || self.convert_target() == Some(target) {
            return None;
        }
        if !target.is_linear() {
            let mut base = self.base().clone();
            bump(&mut base);
            return Some(match target {
                ConvertTarget::Rectangle => Self::Rectangle { base },
                ConvertTarget::Diamond => Self::Diamond { base },
                _ => Self::Ellipse { base },
            });
        }
        let mut linear = match self {
            Self::Line(l) => l.linear.clone(),
            Self::Arrow(a) => a.linear.clone(),
            _ => return None,
        };
        bump(&mut linear.base);
        match target {
            ConvertTarget::Line => {
                let polygon = matches!(self, Self::Line(l) if l.polygon);
                Some(Self::Line(LineElement { linear, polygon }))
            }
            ConvertTarget::SharpArrow => {
                linear.base.roundness = None;
                Some(Self::Arrow(ArrowElement {
                    linear,
                    elbowed: false,
                    fixed_segments: None,
                    start_is_special: None,
                    end_is_special: None,
                }))
            }
            ConvertTarget::CurvedArrow => {
                linear.base.roundness = Some(Roundness {
                    kind: RoundnessType::ProportionalRadius,
                    value: None,
                });
                Some(Self::Arrow(ArrowElement {
                    linear,
                    elbowed: false,
                    fixed_segments: None,
                    start_is_special: None,
                    end_is_special: None,
                }))
            }
            _ => {
                linear.base.roundness = None;
                Some(Self::Arrow(ArrowElement {
                    linear,
                    elbowed: true,
                    fixed_segments: None,
                    start_is_special: None,
                    end_is_special: None,
                }))
            }
        }
    }
}
