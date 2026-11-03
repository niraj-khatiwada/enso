use std::collections::HashMap;

use serde_json::Value;

use crate::{
    core::{
        element::Element,
        scene::{AppState, BinaryFileData, Scene, SceneData},
    },
    render::export::SvgExporter,
};

pub type BinaryFiles = HashMap<String, BinaryFileData>;

#[derive(Debug, Clone)]
pub struct ExcalidrawFile {
    pub elements: Vec<Element>,
    pub app_state: AppState,
}

impl ExcalidrawFile {
    pub fn from_scene(scene: &Scene, app_state: AppState) -> Self {
        Self {
            elements: scene.elements.clone(),
            app_state,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let data = SceneData {
            elements: self.elements.clone(),
            app_state: self.app_state.clone(),
            files: self.app_state.files.clone(),
        };
        serde_json::to_string(&data)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        let data = SceneData {
            elements: self.elements.clone(),
            app_state: self.app_state.clone(),
            files: self.app_state.files.clone(),
        };
        serde_json::to_string_pretty(&data)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let data: SceneData = serde_json::from_str(json)?;
        let mut app_state = data.app_state;
        if !data.files.is_empty() {
            app_state.files = data.files;
        }
        Ok(Self {
            elements: data.elements,
            app_state,
        })
    }

    pub fn into_scene(self) -> (Scene, AppState) {
        let mut scene = Scene::new();
        scene.elements = self.elements;
        (scene, self.app_state)
    }
}

pub fn export_svg(scene: &Scene, background: &str) -> String {
    SvgExporter::export(scene, background)
}

pub fn export_svg_with_files(scene: &Scene, background: &str, files: &BinaryFiles) -> String {
    SvgExporter::export_with_files(scene, background, files)
}

/// Dark mode is applied by the renderer as the upstream `invert(93%)
/// hue-rotate(180deg)` filter, so it has to travel with the export call.
pub fn export_svg_with_dark_mode(
    scene: &Scene,
    background: &str,
    files: &BinaryFiles,
    dark_mode: bool,
) -> String {
    SvgExporter::export_with_options(scene, background, files, dark_mode)
}

pub fn export_png(scene: &Scene, background: &str, scale: f32) -> Result<Vec<u8>, String> {
    crate::render::export::export_png(scene, background, scale)
}

pub fn export_png_with_files(
    scene: &Scene,
    background: &str,
    scale: f32,
    files: &BinaryFiles,
) -> Result<Vec<u8>, String> {
    crate::render::export::export_png_with_files(scene, background, scale, files)
}

pub fn export_png_with_dark_mode(
    scene: &Scene,
    background: &str,
    scale: f32,
    files: &BinaryFiles,
    dark_mode: bool,
) -> Result<Vec<u8>, String> {
    crate::render::export::export_png_with_options(scene, background, scale, files, dark_mode)
}

pub fn export_selected_png(
    scene: &Scene,
    ids: &[String],
    background: &str,
    scale: f32,
) -> Result<Vec<u8>, String> {
    let mut tmp = Scene::new();
    for e in scene.non_deleted() {
        if ids.iter().any(|id| id == e.id()) {
            tmp.add(e.clone());
        }
    }
    crate::render::export::export_png(&tmp, background, scale)
}

pub fn export_selected_svg(scene: &Scene, ids: &[String], background: &str) -> String {
    let mut tmp = Scene::new();
    for e in scene.non_deleted() {
        if ids.iter().any(|id| id == e.id()) {
            tmp.add(e.clone());
        }
    }
    SvgExporter::export(&tmp, background)
}

pub fn export_element_json(element: &Element) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(element)
}

pub fn import_element_json(json: &str) -> Result<Element, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn json_to_scene(json: &str) -> Result<(Scene, AppState), anyhow::Error> {
    let v: Value = serde_json::from_str(json)?;
    let mut app_state: AppState = if v.get("appState").is_some() {
        serde_json::from_value(v["appState"].clone())?
    } else {
        AppState::default()
    };
    if let Some(files) = v.get("files").filter(|f| f.is_object()) {
        app_state.files = serde_json::from_value(files.clone())?;
    }
    let elements: Vec<Element> = if v.get("elements").is_some() {
        serde_json::from_value(v["elements"].clone())?
    } else {
        Vec::new()
    };
    let mut scene = Scene::new();
    scene.elements = elements;
    Ok((scene, app_state))
}

pub fn scene_to_json(scene: &Scene, app_state: &AppState) -> Result<String, anyhow::Error> {
    let mut v = serde_json::to_value(SceneData {
        elements: scene.elements.clone(),
        app_state: app_state.clone(),
        files: app_state.files.clone(),
    })?;
    if let Some(obj) = v.as_object_mut() {
        obj.insert("type".to_string(), Value::String("excalidraw".to_string()));
        obj.insert("version".to_string(), Value::Number(2.into()));
        obj.insert(
            "source".to_string(),
            Value::String("https://excalidraw.com".to_string()),
        );
    }
    Ok(serde_json::to_string_pretty(&v)?)
}
