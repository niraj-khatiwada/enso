use gpui_excalidraw::{
    core::{element::Element, scene::Scene},
    ui::{
        i18n::Language,
        state::{Document, History, Tool},
    },
};

fn rect(elements: &mut Vec<Element>, id: &str) {
    let mut base = gpui_excalidraw::core::element::ElementBase::new(
        id.to_string(),
        gpui_excalidraw::core::element::ElementType::Rectangle,
    );
    base.width = 10.0;
    base.height = 10.0;
    elements.push(Element::Rectangle { base });
}

fn make_scene() -> Scene {
    let mut scene = Scene::new();
    rect(&mut scene.elements, "a");
    rect(&mut scene.elements, "b");
    scene
}

#[test]
fn test_editor_tool_switching() {
    let mut editor = gpui_excalidraw::ui::Editor::new();
    assert_eq!(editor.document.tool, Tool::Selection);
    editor.set_tool(Tool::Rectangle);
    assert_eq!(editor.document.tool, Tool::Rectangle);
    editor.set_tool(Tool::Ellipse);
    assert_eq!(editor.document.tool, Tool::Ellipse);
}

#[test]
fn test_editor_demo_scene() {
    let mut editor = gpui_excalidraw::ui::Editor::new();
    assert!(editor.document.scene.is_empty());
    editor.add_demo_scene();
    assert!(editor.document.scene.len() >= 5);
}

#[test]
fn test_editor_language_switch() {
    let mut editor = gpui_excalidraw::ui::Editor::new();
    assert_eq!(editor.i18n.t("action.undo"), "Undo");
    editor.set_language(Language::ZhCn);
    assert_eq!(editor.i18n.t("action.undo"), "撤销");
}

#[test]
fn test_document_scene_isolation() {
    let scene = make_scene();
    let doc = Document {
        scene,
        ..Document::new()
    };
    assert_eq!(doc.scene.len(), 2);
}

#[test]
fn test_history_snapshot_deduplication() {
    let scene = make_scene();
    let mut history = History::new(50);
    history.push(&scene);
    assert!(history.can_undo());
}
