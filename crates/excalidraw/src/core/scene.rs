use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{element::Element, types::Theme};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryFileData {
    pub id: String,
    pub mime_type: String,
    #[serde(rename = "dataURL")]
    pub data_url: String,
    #[serde(default)]
    pub created: f64,
}

impl BinaryFileData {
    pub fn new(
        id: impl Into<String>,
        mime_type: impl Into<String>,
        data_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            mime_type: mime_type.into(),
            data_url: data_url.into(),
            created: 0.0,
        }
    }

    pub fn base64(&self) -> Option<&str> {
        self.data_url.split_once("base64,").map(|(_, b)| b)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppState {
    pub theme: Theme,
    pub view_background_color: String,
    pub current_item_font_family: i32,
    pub current_item_font_size: f64,
    pub current_item_text_align: String,
    pub current_item_roundness: String,
    pub current_item_roughness: f64,
    pub current_item_stroke_style: String,
    pub current_item_stroke_width: f64,
    pub current_item_fill_style: String,
    pub current_item_stroke_color: String,
    pub current_item_background_color: String,
    pub current_item_opacity: f64,
    pub selected_element_ids: HashMap<String, bool>,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub zoom: Zoom,
    #[serde(skip)]
    pub files: HashMap<String, BinaryFileData>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            view_background_color: "#ffffff".to_string(),
            current_item_font_family: 1,
            current_item_font_size: 20.0,
            current_item_text_align: "left".to_string(),
            current_item_roundness: "round".to_string(),
            current_item_roughness: 1.0,
            current_item_stroke_style: "solid".to_string(),
            current_item_stroke_width: 2.0,
            current_item_fill_style: "hachure".to_string(),
            current_item_stroke_color: "#1e1e1e".to_string(),
            current_item_background_color: "transparent".to_string(),
            current_item_opacity: 100.0,
            selected_element_ids: HashMap::new(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: Zoom::default(),
            files: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Zoom {
    pub value: f64,
}

impl Default for Zoom {
    fn default() -> Self {
        Self { value: 1.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneData {
    pub elements: Vec<Element>,
    #[serde(default)]
    pub app_state: AppState,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub files: HashMap<String, BinaryFileData>,
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub elements: Vec<Element>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id() == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id() == id)
    }

    pub fn add(&mut self, element: Element) {
        if matches!(
            element.kind(),
            crate::element::ElementType::Frame | crate::element::ElementType::MagicFrame
        ) {
            let idx = self
                .elements
                .iter()
                .rposition(|e| {
                    matches!(
                        e.kind(),
                        crate::element::ElementType::Frame
                            | crate::element::ElementType::MagicFrame
                    )
                })
                .map(|i| i + 1)
                .unwrap_or(0);
            self.elements.insert(idx, element);
        } else {
            self.elements.push(element);
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<Element> {
        let idx = self.elements.iter().position(|e| e.id() == id)?;
        Some(self.elements.remove(idx))
    }

    pub fn non_deleted(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(|e| !e.is_deleted())
    }
}
