use gpui_excalidraw::{
    core::{
        element::{BindMode, Element, ElementBase, ElementType},
        scene::{AppState, BinaryFileData, Scene},
        types::{FontFamily, RoundnessType, StrokeVariability},
    },
    ui::export::{ExcalidrawFile, export_svg, json_to_scene, scene_to_json},
};
use serde_json::Value;

const UPSTREAM_EXPORT: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [
    {
      "id": "rect1", "type": "rectangle", "x": 10, "y": 20, "width": 100, "height": 60,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
      "opacity": 100, "groupIds": [], "frameId": null, "roundness": { "type": 3 },
      "seed": 123, "version": 4, "versionNonce": 5, "isDeleted": false,
      "boundElements": null, "updated": 1700000000000, "link": null, "locked": false,
      "index": "a0", "created": 1700000000000
    },
    {
      "id": "text1", "type": "text", "x": 10, "y": 100, "width": 80, "height": 25,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
      "opacity": 100, "groupIds": [], "frameId": null, "roundness": null,
      "seed": 9, "version": 3, "versionNonce": 1, "isDeleted": false,
      "boundElements": null, "updated": 1700000000000, "link": null, "locked": false,
      "index": "a1", "created": 1700000000000,
      "fontSize": 20, "fontFamily": 5, "baseFontSize": null, "text": "hello",
      "textAlign": "left", "verticalAlign": "top", "containerId": null,
      "originalText": "hello", "autoResize": true, "lineHeight": 1.25,
      "labelPosition": null
    },
    {
      "id": "line1", "type": "line", "x": 200, "y": 20, "width": 100, "height": 0,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
      "opacity": 100, "groupIds": [], "frameId": null, "roundness": { "type": 2 },
      "seed": 11, "version": 2, "versionNonce": 3, "isDeleted": false,
      "boundElements": null, "updated": 1700000000000, "link": null, "locked": false,
      "index": "a2", "created": 1700000000000,
      "points": [[0, 0], [100, 0]], "startBinding": null, "endBinding": null,
      "startArrowhead": null, "endArrowhead": null, "polygon": false
    }
  ],
  "appState": {
    "gridSize": 20,
    "gridStep": 5,
    "gridModeEnabled": false,
    "viewBackgroundColor": "#ffffff",
    "lockedMultiSelections": {}
  },
  "files": {}
}"##;

const LEGACY_EXPORT: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [
    {
      "id": "arrow1", "type": "arrow", "x": 0, "y": 0, "width": 100, "height": 0,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
      "opacity": 100, "groupIds": [], "frameId": null, "roundness": { "type": 2 },
      "seed": 1, "version": 2, "versionNonce": 1, "isDeleted": false,
      "boundElements": null, "updated": 1, "link": null, "locked": false,
      "index": "a0", "created": 1,
      "points": [[0, 0], [100, 0]],
      "startBinding": { "elementId": "rect1", "focus": 0, "gap": 4 },
      "endBinding": null,
      "startArrowhead": null, "endArrowhead": "arrow"
    },
    {
      "id": "fd1", "type": "freedraw", "x": 0, "y": 0, "width": 1, "height": 1,
      "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
      "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
      "opacity": 100, "groupIds": [], "frameId": null, "roundness": null,
      "seed": 2, "version": 2, "versionNonce": 2, "isDeleted": false,
      "boundElements": null, "updated": 1, "link": null, "locked": false,
      "index": "a1", "created": 1,
      "points": [[0, 0], [1, 1]], "pressures": [0.5, 0.5],
      "simulatePressure": true, "strokeOptions": { "variability": "variable" }
    }
  ],
  "appState": { "viewBackgroundColor": "#ffffff" },
  "files": {}
}"##;

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = ElementBase::new(id.to_string(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    base.seed = 7;
    Element::Rectangle { base }
}

fn scene_with_elements() -> Scene {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    scene.add(rect("b", 100.0, 100.0, 50.0, 50.0));
    scene
}

#[test]
fn test_excalidraw_file_roundtrip() {
    let scene = scene_with_elements();
    let file = ExcalidrawFile::from_scene(&scene, AppState::default());
    let json = file.to_json().unwrap();
    let back = ExcalidrawFile::from_json(&json).unwrap();
    assert_eq!(back.elements.len(), 2);
    assert_eq!(back.elements[0].id(), "a");
}

#[test]
fn test_export_svg() {
    let scene = scene_with_elements();
    let svg = export_svg(&scene, "#ffffff");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<path"));
}

#[test]
fn test_export_svg_embeds_the_hand_drawn_font() {
    use gpui_excalidraw::core::factory::{ElementOptions, new_text};

    let mut scene = Scene::new();
    scene.add(new_text(
        0.0,
        0.0,
        "Hello",
        20.0,
        &ElementOptions::default(),
    ));

    let svg = export_svg(&scene, "#ffffff");
    assert!(
        svg.contains("@font-face") && svg.contains("font-family:'Excalifont'"),
        "the export must carry the Excalifont face so viewers outside this app still draw it"
    );
    assert!(
        svg.contains("font-family=\"Excalifont, sans-serif\""),
        "text must ask for the hand-drawn family, not a system fallback"
    );
}

#[test]
fn test_scene_to_json_has_excalidraw_metadata() {
    let scene = scene_with_elements();
    let json = scene_to_json(&scene, &AppState::default()).unwrap();
    assert!(json.contains("\"type\": \"excalidraw\""));
    assert!(json.contains("\"version\": 2"));
    assert!(json.contains("\"elements\""));
    assert!(json.contains("\"appState\""));
}

#[test]
fn test_json_to_scene_roundtrip() {
    let scene = scene_with_elements();
    let json = scene_to_json(&scene, &AppState::default()).unwrap();
    let (parsed, state) = json_to_scene(&json).unwrap();
    assert_eq!(parsed.len(), 2);
    let _ = state;
}

#[test]
fn test_files_are_stored_at_the_json_top_level() {
    let scene = scene_with_elements();
    let mut state = AppState::default();
    state.files.insert(
        "file-1".to_string(),
        BinaryFileData::new("file-1", "image/png", "data:image/png;base64,AAAA"),
    );

    let json = scene_to_json(&scene, &state).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert!(v.get("files").and_then(|f| f.as_object()).is_some());
    assert!(
        v["appState"].get("files").is_none(),
        "files must not nest inside appState"
    );

    let (_, back) = json_to_scene(&json).unwrap();
    assert_eq!(back.files.len(), 1);

    let file = ExcalidrawFile::from_scene(&scene, state);
    let json = file.to_json().unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert!(v.get("files").and_then(|f| f.as_object()).is_some());
    assert!(v["appState"].get("files").is_none());
    let back = ExcalidrawFile::from_json(&json).unwrap();
    assert_eq!(back.app_state.files.len(), 1);
}

#[test]
fn test_json_to_scene_empty() {
    let (scene, _state) = json_to_scene(r#"{"elements":[]}"#).unwrap();
    assert!(scene.is_empty());
}

#[test]
fn test_upstream_export_loads() {
    let (scene, state) = json_to_scene(UPSTREAM_EXPORT).expect("a real file must open");
    assert_eq!(scene.len(), 3);
    assert_eq!(state.view_background_color, "#ffffff");

    assert_eq!(state.theme, gpui_excalidraw::core::types::Theme::Light);
    assert_eq!(state.zoom.value, 1.0);
}

#[test]
fn test_upstream_encodings_survive_a_round_trip() {
    let (scene, state) = json_to_scene(UPSTREAM_EXPORT).unwrap();

    match scene.get("rect1").unwrap() {
        Element::Rectangle { base } => {
            assert_eq!(
                base.roundness.map(|r| r.kind),
                Some(RoundnessType::AdaptiveRadius),
                "roundness.type is a number upstream, not a name"
            );
        }
        other => panic!("expected rectangle, got {:?}", other.kind()),
    }

    match scene.get("text1").unwrap() {
        Element::Text(t) => assert_eq!(t.font_family, FontFamily::Excalifont),
        other => panic!("expected text, got {:?}", other.kind()),
    }

    match scene.get("line1").unwrap() {
        Element::Line(l) => {
            assert_eq!(l.linear.points.len(), 2);
            assert_eq!(
                l.linear.points[1],
                gpui_excalidraw::core::geometry::Point::new(100.0, 0.0)
            );
        }
        other => panic!("expected line, got {:?}", other.kind()),
    }

    let json = scene_to_json(&scene, &state).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    let elements = v["elements"].as_array().unwrap();
    let by_id = |id: &str| {
        elements
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing"))
    };
    assert_eq!(by_id("rect1")["roundness"]["type"], 3);
    assert_eq!(by_id("text1")["fontFamily"], 5);
    assert_eq!(by_id("line1")["points"][1], serde_json::json!([100.0, 0.0]));
}

#[test]
fn test_legacy_bindings_and_stroke_options_load() {
    let (scene, _state) = json_to_scene(LEGACY_EXPORT).expect("a legacy file must open");
    assert_eq!(scene.len(), 2);

    match scene.get("arrow1").unwrap() {
        Element::Arrow(a) => {
            let binding = a.linear.start_binding.as_ref().expect("binding kept");
            assert_eq!(binding.element_id, "rect1");
            assert_eq!(binding.mode, BindMode::Orbit);
            assert_eq!(binding.fixed_point, [0.5001, 0.5001]);
        }
        other => panic!("expected arrow, got {:?}", other.kind()),
    }

    match scene.get("fd1").unwrap() {
        Element::Freedraw(f) => {
            assert_eq!(f.stroke_options.streamline, 0.3);
            assert_eq!(f.stroke_options.variability, StrokeVariability::Variable);
        }
        other => panic!("expected freedraw, got {:?}", other.kind()),
    }
}
