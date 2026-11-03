use gpui_excalidraw::{
    core::element::Element,
    ui::{Editor, state::Tool},
};
use gpui_kit::Modifiers;

fn plain() -> Modifiers {
    Modifiers::default()
}

fn with_alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Default::default()
    }
}

fn with_shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

fn with_ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = gpui_excalidraw::core::element::ElementBase::new(
        id.to_string(),
        gpui_excalidraw::core::element::ElementType::Rectangle,
    );
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    Element::Rectangle { base }
}

const VP: (f64, f64) = (800.0, 600.0);

#[test]
fn multi_point_line_drag_commits_two_points() {
    let mut e = Editor::new();
    e.set_tool(Tool::Line);
    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    e.on_canvas_mouse_move(200.0, 10.0, plain());
    e.on_canvas_mouse_up(200.0, 10.0, plain());
    assert_eq!(e.document.scene.len(), 1);
    match &e.document.scene.elements[0] {
        Element::Line(l) => assert_eq!(l.linear.points.len(), 2),
        other => panic!("expected line, got {:?}", other.kind()),
    }
}

#[test]
fn multi_point_line_click_click_enter() {
    let mut e = Editor::new();
    e.set_tool(Tool::Line);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    e.on_canvas_mouse_up(0.0, 0.0, plain());
    assert!(
        e.document.scene.is_empty(),
        "a single click must not commit"
    );
    assert_eq!(e.pending_point_count(), 1);

    e.on_canvas_mouse_down(100.0, 0.0, plain(), 1);
    e.on_canvas_mouse_up(100.0, 0.0, plain());
    assert!(e.document.scene.is_empty(), "still editing");
    assert_eq!(e.pending_point_count(), 2);

    assert!(e.handle_key("enter", None, plain(), VP));
    assert_eq!(e.document.scene.len(), 1);
    assert_eq!(e.pending_point_count(), 0);
}

#[test]
fn multi_point_line_three_vertices() {
    let mut e = Editor::new();
    e.set_tool(Tool::Arrow);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    e.on_canvas_mouse_up(0.0, 0.0, plain());
    e.on_canvas_mouse_down(100.0, 0.0, plain(), 1);
    e.on_canvas_mouse_up(100.0, 0.0, plain());
    e.on_canvas_mouse_down(100.0, 100.0, plain(), 1);
    e.on_canvas_mouse_up(100.0, 100.0, plain());
    e.handle_key("enter", None, plain(), VP);
    match &e.document.scene.elements[0] {
        Element::Arrow(a) => assert_eq!(a.linear.points.len(), 3),
        other => panic!("expected arrow, got {:?}", other.kind()),
    }
}

#[test]
fn escape_discards_pending_line() {
    let mut e = Editor::new();
    e.set_tool(Tool::Line);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    e.on_canvas_mouse_up(0.0, 0.0, plain());
    e.handle_key("escape", None, plain(), VP);
    assert!(e.document.scene.is_empty());
    assert_eq!(e.pending_point_count(), 0);
}

#[test]
fn arrow_drag_binds_to_shape() {
    let mut e = Editor::new();
    e.document.scene.add(rect("r", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Arrow);
    e.on_canvas_mouse_down(200.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(100.0, 50.0, plain());
    e.on_canvas_mouse_up(100.0, 50.0, plain());
    assert_eq!(e.document.scene.len(), 2);
    let arrow = e
        .document
        .scene
        .elements
        .iter()
        .find(|el| matches!(el, Element::Arrow(_)))
        .expect("arrow created");
    if let Element::Arrow(a) = arrow {
        assert!(
            a.linear.end_binding.is_some(),
            "end should bind to the rect"
        );
        assert_eq!(a.linear.end_binding.as_ref().unwrap().element_id, "r");
    }
}

#[test]
fn moving_a_shape_drags_its_bound_arrow() {
    let mut e = Editor::new();
    e.document.scene.add(rect("r", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Arrow);
    e.on_canvas_mouse_down(200.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(100.0, 50.0, plain());
    e.on_canvas_mouse_up(100.0, 50.0, plain());
    let arrow_id = e
        .document
        .scene
        .elements
        .iter()
        .find(|el| matches!(el, Element::Arrow(_)))
        .unwrap()
        .id()
        .to_string();

    e.set_tool(Tool::Selection);
    e.document.select("r");
    e.on_canvas_mouse_down(50.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(150.0, 50.0, plain());
    e.on_canvas_mouse_up(150.0, 50.0, plain());

    if let Element::Arrow(a) = e.document.scene.get(&arrow_id).unwrap() {
        assert!(
            (a.linear.points[1].x - 200.0).abs() < 1e-6,
            "bound endpoint should follow the shape, got {}",
            a.linear.points[1].x
        );
    } else {
        panic!("expected arrow");
    }
}

#[test]
fn alt_drag_duplicates_selection() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(50.0, 50.0, with_alt(), 1);
    e.on_canvas_mouse_move(90.0, 50.0, with_alt());
    e.on_canvas_mouse_up(90.0, 50.0, with_alt());
    assert_eq!(e.document.scene.len(), 2, "alt-drag duplicates");
}

#[test]
fn plain_drag_does_not_duplicate() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(50.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(90.0, 50.0, plain());
    e.on_canvas_mouse_up(90.0, 50.0, plain());
    assert_eq!(e.document.scene.len(), 1);
    assert_eq!(e.document.scene.get("a").unwrap().base().x, 40.0);
}

#[test]
fn shift_drag_creates_square() {
    let mut e = Editor::new();
    e.set_tool(Tool::Rectangle);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    e.on_canvas_mouse_move(100.0, 40.0, with_shift());
    e.on_canvas_mouse_up(100.0, 40.0, with_shift());
    let base = e.document.scene.elements[0].base();
    assert_eq!(base.width, base.height);
    assert_eq!(base.width, 100.0);
}

#[test]
fn bare_click_creates_default_sized_shape() {
    let mut e = Editor::new();
    e.set_tool(Tool::Rectangle);
    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    e.on_canvas_mouse_up(10.0, 10.0, plain());
    let base = e.document.scene.elements[0].base();
    assert_eq!(base.width, 100.0);
    assert_eq!(base.height, 100.0);
}

#[test]
fn grid_snap_quantises_movement() {
    let mut e = Editor::new();
    e.document.snap_enabled = true;
    e.document.grid_size = 20.0;
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(50.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(57.0, 68.0, plain());
    e.on_canvas_mouse_up(57.0, 68.0, plain());
    let base = e.document.scene.get("a").unwrap().base();
    assert_eq!((base.x, base.y), (0.0, 20.0));
}

#[test]
fn eraser_drag_removes_several_elements() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 50.0, 50.0));
    e.document.scene.add(rect("b", 100.0, 0.0, 50.0, 50.0));
    e.set_tool(Tool::Eraser);
    e.on_canvas_mouse_down(25.0, 25.0, plain(), 1);
    e.on_canvas_mouse_move(125.0, 25.0, plain());
    e.on_canvas_mouse_up(125.0, 25.0, plain());
    assert!(e.document.scene.is_empty(), "both should be erased");
}

#[test]
fn marquee_selects_everything_it_touches() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 50.0, 50.0));
    e.document.scene.add(rect("b", 200.0, 200.0, 50.0, 50.0));
    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(-50.0, -50.0, plain(), 1);
    e.on_canvas_mouse_move(100.0, 100.0, plain());
    assert_eq!(e.document.selected.len(), 1);
    assert!(e.document.selected.contains("a"));
    e.on_canvas_mouse_up(100.0, 100.0, plain());
}

#[test]
fn lasso_selects_only_fully_enclosed_shapes() {
    let mut e = Editor::new();
    e.document.scene.add(rect("inside", 0.0, 0.0, 50.0, 50.0));
    e.document
        .scene
        .add(rect("outside", 200.0, 200.0, 50.0, 50.0));
    e.set_tool(Tool::Lasso);
    e.on_canvas_mouse_down(-20.0, -20.0, plain(), 1);
    e.on_canvas_mouse_move(80.0, -20.0, plain());
    e.on_canvas_mouse_move(80.0, 80.0, plain());
    e.on_canvas_mouse_move(-20.0, 80.0, plain());
    assert_eq!(
        e.document.selected.len(),
        1,
        "only the fully enclosed shape should be selected"
    );
    assert!(e.document.selected.contains("inside"));
    e.on_canvas_mouse_up(-20.0, 80.0, plain());
    assert_eq!(
        e.document.selected.len(),
        1,
        "releasing keeps the selection"
    );
    assert_eq!(e.document.tool, Tool::Lasso, "the tool stays active");
}

#[test]
fn lasso_ignores_partially_covered_shapes() {
    let mut e = Editor::new();
    e.document
        .scene
        .add(rect("straddle", 30.0, 0.0, 50.0, 50.0));
    e.set_tool(Tool::Lasso);
    e.on_canvas_mouse_down(-20.0, -20.0, plain(), 1);
    e.on_canvas_mouse_move(60.0, -20.0, plain());
    e.on_canvas_mouse_move(60.0, 60.0, plain());
    e.on_canvas_mouse_move(-20.0, 60.0, plain());
    assert!(
        e.document.selected.is_empty(),
        "a shape the path crosses must not be selected"
    );
    e.on_canvas_mouse_up(-20.0, 60.0, plain());
}

#[test]
fn lasso_press_on_the_selection_drags_it() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 50.0, 50.0));
    e.set_tool(Tool::Lasso);
    e.document.select("a");
    e.on_canvas_mouse_down(25.0, 25.0, plain(), 1);
    e.on_canvas_mouse_move(45.0, 25.0, plain());
    e.on_canvas_mouse_up(45.0, 25.0, plain());
    let base = e.document.scene.get("a").unwrap().base();
    assert_eq!(base.x, 20.0, "the selected shape should follow the drag");
    assert!(e.document.selected.contains("a"));
}

#[test]
fn keyboard_copy_paste_roundtrip() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    assert!(e.handle_key("a", None, with_ctrl(), VP));
    assert_eq!(e.document.selected.len(), 1);
    assert!(e.handle_key("c", None, with_ctrl(), VP));
    assert!(e.document.has_clipboard());
    assert!(e.handle_key("v", None, with_ctrl(), VP));
    assert_eq!(e.document.scene.len(), 2);
}

#[test]
fn cut_removes_the_source_element() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    e.handle_key("a", None, with_ctrl(), VP);
    e.handle_key("x", None, with_ctrl(), VP);
    assert!(e.document.scene.is_empty());
    assert!(e.document.has_clipboard());
}

#[test]
fn shift_digit_keys_zoom_to_fit() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 1000.0, 1000.0));
    assert!(e.handle_key("1", None, with_shift(), VP));
    assert!(e.document.zoom < 1.0, "zoom-to-fit should zoom out");
}

#[test]
fn tool_shortcuts_switch_tools() {
    let mut e = Editor::new();
    e.handle_key("r", None, plain(), VP);
    assert_eq!(e.document.tool, Tool::Rectangle);
    e.handle_key("o", None, plain(), VP);
    assert_eq!(e.document.tool, Tool::Ellipse);
    e.handle_key("a", None, plain(), VP);
    assert_eq!(e.document.tool, Tool::Arrow);
    e.handle_key("v", None, plain(), VP);
    assert_eq!(e.document.tool, Tool::Selection);
}

#[test]
fn typing_replaces_text_placeholder() {
    let mut e = Editor::new();
    e.set_tool(Tool::Text);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    let id = e
        .editing_text_id()
        .expect("text editing started")
        .to_string();
    if let Element::Text(t) = e.document.scene.get(&id).unwrap() {
        assert_eq!(t.text, "Text");
    }
    e.handle_key("h", Some("h".to_string()), plain(), VP);
    if let Element::Text(t) = e.document.scene.get(&id).unwrap() {
        assert_eq!(t.text, "h", "placeholder must be replaced on first key");
    }
    e.handle_key("i", Some("i".to_string()), plain(), VP);
    if let Element::Text(t) = e.document.scene.get(&id).unwrap() {
        assert_eq!(t.text, "hi");
    }

    if let Element::Text(t) = e.document.scene.get(&id).unwrap() {
        assert!(t.base.width > 0.0 && t.base.height > 0.0);
    }
}

#[test]
fn text_creation_uses_style_font_size_not_stroke_width() {
    let mut e = Editor::new();
    e.document.style.font_size = 28.0;
    e.document.style.stroke_width = 6.0;
    e.set_tool(Tool::Text);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    let id = e.editing_text_id().unwrap().to_string();
    if let Element::Text(t) = e.document.scene.get(&id).unwrap() {
        assert_eq!(t.font_size, 28.0);
    }
}

#[test]
fn save_and_load_roundtrip() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    e.document.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    let path = std::env::temp_dir().join("gpui_excalidraw_roundtrip.excalidraw");
    let path = path.to_string_lossy().to_string();
    e.save_to(&path).expect("save");

    let mut e2 = Editor::new();
    e2.load_from(&path).expect("load");
    assert_eq!(e2.document.scene.len(), 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn open_requests_a_file_picker() {
    let mut e = Editor::new();
    assert!(!e.scene_open_pending());
    assert!(e.handle_key("o", None, with_ctrl(), VP));
    assert!(
        e.scene_open_pending(),
        "Ctrl+O has to ask for a file, not read a fixed path"
    );

    let mut e = Editor::new();
    e.handle_key("o", None, plain(), VP);
    assert_eq!(e.document.tool, Tool::Ellipse);
    assert!(!e.scene_open_pending());
}

#[test]
fn save_requests_the_export_dialog() {
    let mut e = Editor::new();
    assert!(!e.save_dialog_pending());
    assert!(e.handle_key("s", None, with_ctrl(), VP));
    assert!(
        e.save_dialog_pending(),
        "Ctrl+S has to raise the dialog rather than write a fixed path"
    );
    assert!(!e.save_dialog_open(), "the modal opens on the next frame");
    assert_eq!(e.status_text(), None, "nothing may be written yet");
}

#[test]
fn export_dialog_seeds_an_untitled_name() {
    let mut e = Editor::new();
    e.open_save_dialog();
    assert!(e.save_dialog_open());

    let name = e.project_name_value();
    assert!(
        name.starts_with("Untitled-"),
        "App.getName() prefixes labels.untitled, got {name:?}"
    );
    let stamp = &name["Untitled-".len()..];
    assert_eq!(stamp.len(), 15, "unexpected timestamp {stamp:?}");
    assert_eq!(stamp.as_bytes()[4], b'-');
    assert_eq!(stamp.as_bytes()[7], b'-');
    assert_eq!(stamp.as_bytes()[10], b'-');
    assert_eq!(stamp.chars().filter(char::is_ascii_digit).count(), 12);
}

#[test]
fn export_dialog_owns_the_keyboard() {
    let mut e = Editor::new();
    e.open_save_dialog();
    let seeded = e.project_name_value();
    let tool = e.document.tool;

    assert!(e.handle_key("backspace", None, plain(), VP));
    assert_eq!(e.project_name_value().len(), seeded.len() - 1);
    assert!(e.handle_key("x", Some("x".to_string()), plain(), VP));
    assert!(e.project_name_value().ends_with('x'));
    assert_eq!(
        e.document.tool, tool,
        "the letter must not reach the canvas"
    );

    assert!(e.handle_key("escape", None, plain(), VP));
    assert!(!e.save_dialog_open(), "Escape dismisses the dialog");
}

#[test]
fn export_dialog_edits_at_the_caret() {
    let mut e = Editor::new();
    e.open_save_dialog();
    let seeded = e.project_name_value();

    // Home parks the caret at the start, so typing there prepends.
    assert!(e.handle_key("home", None, plain(), VP));
    assert!(e.handle_key("A", Some("A".to_string()), plain(), VP));
    assert_eq!(e.project_name_value(), format!("A{seeded}"));

    // End parks it at the end, so typing there appends.
    assert!(e.handle_key("end", None, plain(), VP));
    assert!(e.handle_key("B", Some("B".to_string()), plain(), VP));
    assert_eq!(e.project_name_value(), format!("A{seeded}B"));

    // Delete removes the character after the caret; backspace the one before.
    assert!(e.handle_key("home", None, plain(), VP));
    assert!(e.handle_key("delete", None, plain(), VP));
    assert_eq!(e.project_name_value(), format!("{seeded}B"));
    assert!(e.handle_key("end", None, plain(), VP));
    assert!(e.handle_key("backspace", None, plain(), VP));
    assert_eq!(e.project_name_value(), seeded);

    // Left/right step one character at a time.
    assert!(e.handle_key("left", None, plain(), VP));
    assert!(e.handle_key("C", Some("C".to_string()), plain(), VP));
    let split = seeded.len() - 1;
    assert_eq!(
        e.project_name_value(),
        format!("{}{}{}", &seeded[..split], "C", &seeded[split..])
    );
}

#[test]
fn export_dialog_select_all_replaces_the_name() {
    let mut e = Editor::new();
    e.open_save_dialog();

    assert!(e.handle_key("a", None, with_ctrl(), VP));
    assert!(e.handle_key("Z", Some("Z".to_string()), plain(), VP));
    assert_eq!(
        e.project_name_value(),
        "Z",
        "typing must replace the selection"
    );

    // Left collapses the selection to its start rather than stepping a char.
    let mut e = Editor::new();
    e.open_save_dialog();
    assert!(e.handle_key("a", None, with_ctrl(), VP));
    assert!(e.handle_key("left", None, plain(), VP));
    assert!(e.handle_key("Z", Some("Z".to_string()), plain(), VP));
    assert!(e.project_name_value().starts_with('Z'));
    assert!(
        !e.project_name_value().ends_with('Z'),
        "the selection survived"
    );
}

#[test]
fn export_dialog_caret_respects_character_boundaries() {
    let mut e = Editor::new();
    e.open_save_dialog();
    let seeded = e.project_name_value();

    assert!(e.handle_key("home", None, plain(), VP));
    assert!(e.handle_key("中", Some("中".to_string()), plain(), VP));
    assert_eq!(e.project_name_value(), format!("中{seeded}"));

    // The caret sits after a three-byte character; backspace has to remove the
    // whole character instead of a single byte.
    assert!(e.handle_key("backspace", None, plain(), VP));
    assert_eq!(e.project_name_value(), seeded);

    assert!(e.handle_key("home", None, plain(), VP));
    assert!(e.handle_key("right", None, plain(), VP));
    assert!(e.handle_key("left", None, plain(), VP));
    assert!(e.handle_key("delete", None, plain(), VP));
    assert_eq!(e.project_name_value(), &seeded[1..]);
}

#[test]
fn export_dialog_saves_through_the_picked_path() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    e.open_save_dialog();
    let path = std::env::temp_dir().join("gpui_excalidraw_save_dialog.excalidraw");
    let path = path.to_string_lossy().to_string();

    e.save_scene_file(&path);
    assert!(!e.save_dialog_open(), "a successful save closes the modal");
    assert!(std::path::Path::new(&path).exists());

    let mut e2 = Editor::new();
    e2.open_save_dialog();
    let unwritable = std::env::temp_dir()
        .join("gpui-excalidraw-no-such-directory")
        .join("scene.excalidraw");
    e2.save_scene_file(&unwritable.to_string_lossy());
    assert!(
        e2.save_dialog_open(),
        "a failed save must not close the modal"
    );
    assert!(
        e2.status_text()
            .is_some_and(|s| s.starts_with("save failed:")),
        "a failed save must be reported, got {:?}",
        e2.status_text()
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_image_shortcut_requests_the_dialog() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    let mut mods = with_ctrl();
    mods.shift = true;

    assert!(!e.image_dialog_pending());
    assert!(e.handle_key("e", None, mods, VP));
    assert!(
        e.image_dialog_pending(),
        "Ctrl+Shift+E has to raise the dialog instead of writing a fixed path"
    );
    assert!(!e.image_dialog_open(), "the modal opens on the next frame");
    assert_eq!(e.status_text(), None, "nothing may be written yet");
}

#[test]
fn export_image_dialog_defaults_and_dismissal() {
    let mut e = Editor::new();
    e.open_image_dialog();
    assert!(e.image_dialog_open());
    assert!(e.project_name_value().starts_with("Untitled-"));
    assert_eq!(e.export_scale(), 1.0);
    assert!(e.export_with_background());
    assert!(!e.export_dark_mode());
    assert!(!e.export_embed_scene());
    assert!(!e.export_only_selected());

    assert!(e.handle_key("escape", None, plain(), VP));
    assert!(!e.image_dialog_open(), "Escape dismisses the modal");
}

#[test]
fn export_image_dialog_owns_the_keyboard() {
    let mut e = Editor::new();
    e.open_image_dialog();
    let seeded = e.project_name_value();
    let tool = e.document.tool;

    assert!(e.handle_key("backspace", None, plain(), VP));
    assert_eq!(e.project_name_value().len(), seeded.len() - 1);
    assert!(e.handle_key("x", Some("x".to_string()), plain(), VP));
    assert!(e.project_name_value().ends_with('x'));
    assert_eq!(
        e.document.tool, tool,
        "the letter must not reach the canvas"
    );
}

#[test]
fn loading_an_unreadable_file_reports_it() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    let missing = std::env::temp_dir().join("gpui_excalidraw_does_not_exist.excalidraw");
    let _ = std::fs::remove_file(&missing);

    e.load_scene_file(&missing.to_string_lossy());
    assert!(
        e.status_text()
            .is_some_and(|s| s.starts_with("open failed:")),
        "a failed open must be reported, got {:?}",
        e.status_text()
    );
    assert_eq!(
        e.document.scene.len(),
        1,
        "the scene must survive a failed open"
    );
}

#[test]
fn svg_export_contains_shapes() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    let svg = e.export_svg_string();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("<path"));
}

#[test]
fn undo_after_move_restores_position() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(50.0, 50.0, plain(), 1);
    e.on_canvas_mouse_move(150.0, 50.0, plain());
    e.on_canvas_mouse_up(150.0, 50.0, plain());
    assert_eq!(e.document.scene.get("a").unwrap().base().x, 100.0);
    assert!(e.history.can_undo(), "a move must be undoable");
    e.on_undo();
    assert_eq!(e.document.scene.get("a").unwrap().base().x, 0.0);
}

#[test]
fn rotation_follows_drag_direction() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.document.select("a");

    e.on_canvas_mouse_down(50.0, -24.0, plain(), 1);
    e.on_canvas_mouse_move(150.0, 50.0, plain());

    let angle = e.document.scene.get("a").unwrap().base().angle;
    let quarter = std::f64::consts::FRAC_PI_2;
    assert!(
        (angle - quarter).abs() < 1e-6,
        "expected +90 degrees, got {} degrees",
        angle.to_degrees()
    );
    e.on_canvas_mouse_up(150.0, 50.0, plain());
}

#[test]
fn rotate_handle_appears_above_the_selection() {
    let mut e = Editor::new();
    e.document.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    e.document.select("a");

    e.on_canvas_mouse_down(50.0, -80.0, plain(), 1);
    e.on_canvas_mouse_move(150.0, 50.0, plain());
    let angle = e.document.scene.get("a").unwrap().base().angle;
    assert_eq!(angle, 0.0);
}

#[test]
fn drawing_one_shape_hands_back_the_selection_tool() {
    let mut e = Editor::new();
    assert!(!e.document.tool_locked, "upstream default is unlocked");

    e.set_tool(Tool::Rectangle);
    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    e.on_canvas_mouse_move(120.0, 90.0, plain());
    e.on_canvas_mouse_up(120.0, 90.0, plain());

    assert_eq!(e.document.scene.len(), 1, "the shape must still be created");
    assert_eq!(
        e.document.tool,
        Tool::Selection,
        "the tool should fall back to Selection once the shape is committed"
    );
}

#[test]
fn locking_the_tool_keeps_it_armed() {
    let mut e = Editor::new();
    e.document.tool_locked = true;
    e.set_tool(Tool::Ellipse);

    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    e.on_canvas_mouse_move(120.0, 90.0, plain());
    e.on_canvas_mouse_up(120.0, 90.0, plain());

    assert_eq!(e.document.tool, Tool::Ellipse);
}

#[test]
fn tool_lock_does_not_introduce_a_checkpoint() {
    let mut e = Editor::new();
    e.document.tool_locked = true;
    e.set_tool(Tool::Rectangle);
    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    e.on_canvas_mouse_up(60.0, 60.0, plain());

    assert_eq!(e.document.tool, Tool::Rectangle);
    e.on_undo();
    assert!(
        e.document.scene.is_empty(),
        "one undo should remove the single shape we drew"
    );
}

#[test]
fn dragging_a_rectangle_previews_the_exact_shape_it_commits() {
    use gpui_excalidraw::core::bounds::element_bounds;

    let mut e = Editor::new();
    e.set_tool(Tool::Rectangle);
    assert!(
        e.drag_preview().is_none(),
        "nothing to preview before the press"
    );

    e.on_canvas_mouse_down(40.0, 60.0, plain(), 1);
    let on_press = e
        .drag_preview()
        .expect("a preview exists the moment you press");
    let pressed = element_bounds(&on_press);
    assert!(
        pressed.width() > 0.0 && pressed.height() > 0.0,
        "even a bare press previews the default-size shape, got {pressed:?}"
    );

    e.on_canvas_mouse_move(180.0, 140.0, plain());
    let dragged = element_bounds(&e.drag_preview().expect("still dragging"));

    e.on_canvas_mouse_up(180.0, 140.0, plain());
    assert!(e.drag_preview().is_none(), "the preview retires on release");
    assert_eq!(e.document.scene.len(), 1);
    let committed = element_bounds(e.document.scene.non_deleted().next().unwrap());

    assert!(
        (committed.width() - dragged.width()).abs() < 0.001
            && (committed.height() - dragged.height()).abs() < 0.001,
        "preview {:?} != committed {:?}",
        dragged,
        committed
    );
    assert!(
        dragged.width() >= 120.0,
        "the preview must follow the pointer, not stay at the default size: {dragged:?}"
    );
}

#[test]
fn shift_squares_the_preview_while_the_button_is_still_down() {
    use gpui_excalidraw::core::bounds::element_bounds;

    let mut e = Editor::new();
    e.set_tool(Tool::Ellipse);
    e.on_canvas_mouse_down(0.0, 0.0, plain(), 1);
    e.on_canvas_mouse_move(200.0, 60.0, with_shift());

    let b = element_bounds(&e.drag_preview().expect("previewing"));
    assert!(
        (b.width() - b.height()).abs() < 0.001,
        "shift must square the live preview too, got {b:?}"
    );
}

#[test]
fn a_free_draw_stroke_previews_its_ink_while_the_button_is_held() {
    let mut e = Editor::new();
    e.set_tool(Tool::FreeDraw);
    assert!(e.freedraw_preview().is_none());

    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    assert!(
        e.freedraw_preview().is_none(),
        "a single point is not a stroke yet"
    );
    e.on_canvas_mouse_move(30.0, 24.0, plain());
    assert_eq!(
        e.freedraw_preview().map(|p| p.len()),
        Some(2),
        "the ink trail must be visible mid-stroke"
    );

    e.on_canvas_mouse_up(30.0, 24.0, plain());
    assert!(e.freedraw_preview().is_none(), "the trail is committed");
    assert_eq!(e.document.scene.len(), 1);
}

#[test]
fn the_editor_reports_whether_a_pointer_gesture_is_in_flight() {
    let mut e = Editor::new();
    assert!(!e.is_interacting(), "idle");

    e.set_tool(Tool::Rectangle);
    e.on_canvas_mouse_down(10.0, 10.0, plain(), 1);
    assert!(e.is_interacting(), "a drag is in flight");
    e.on_canvas_mouse_up(80.0, 80.0, plain());
    assert!(!e.is_interacting(), "and ends when the button comes up");

    e.set_tool(Tool::Selection);
    e.on_canvas_mouse_down(200.0, 200.0, plain(), 1);
    assert!(e.is_interacting(), "a marquee is a gesture too");
    e.on_canvas_mouse_move(260.0, 260.0, plain());
    e.on_canvas_mouse_up(260.0, 260.0, plain());
    assert!(!e.is_interacting());
}

#[test]
fn committing_a_freedraw_stroke_keeps_the_rest_of_the_scene() {
    let mut e = Editor::new();
    e.add_demo_scene();
    let before = e.document.scene.len();

    e.set_tool(Tool::FreeDraw);
    e.on_canvas_mouse_down(430.0, 620.0, plain(), 1);
    for i in 0..24 {
        let t = i as f64 / 23.0;
        let step = 34.0 - 26.0 * t;
        let x = 430.0 + (i as f64 + 1.0) * step;
        let y = 620.0 - (t * 8.0).sin() * 60.0 - t * 40.0;
        e.on_canvas_mouse_move(x as f32, y as f32, plain());
    }
    e.on_canvas_mouse_up(790.0, 540.0, plain());

    let total = e.document.scene.len();
    let kinds: Vec<String> = e
        .document
        .scene
        .non_deleted()
        .map(|el| format!("{:?}", el.kind()))
        .collect();
    eprintln!("DIAG before={before} total={total} kinds={kinds:?}");
    assert_eq!(
        total,
        before + 1,
        "the commit must add exactly the stroke, not disturb the scene"
    );
}

#[test]
fn double_clicking_a_sticky_note_edits_its_text() {
    let mut e = Editor::new();
    e.set_tool(Tool::StickyNote);
    e.on_canvas_mouse_down(100.0, 100.0, plain(), 1);
    e.on_canvas_mouse_up(260.0, 240.0, plain());

    let note_id = e
        .document
        .scene
        .elements
        .iter()
        .find(|el| matches!(el, Element::StickyNote(_)))
        .map(|el| el.id().to_string())
        .expect("the sticky note was created");
    let text_id = e
        .document
        .container_text_id(&note_id)
        .expect("the note carries a bound text");

    e.on_canvas_mouse_down(200.0, 120.0, plain(), 2);
    assert_eq!(
        e.editing_text_id(),
        Some(text_id.as_str()),
        "double-clicking the note should edit its text"
    );

    e.handle_key("x", Some("x".into()), plain(), VP);
    let content = match e.document.scene.get(&text_id) {
        Some(Element::Text(t)) => t.text.clone(),
        _ => panic!("the note's text element vanished"),
    };

    assert_eq!(content, "x", "typing should land in the note's text");
}

#[test]
fn clicking_a_sticky_note_selects_the_note_not_its_label() {
    let mut e = Editor::new();
    e.set_tool(Tool::StickyNote);
    e.on_canvas_mouse_down(100.0, 100.0, plain(), 1);
    e.on_canvas_mouse_up(260.0, 240.0, plain());

    let note_id = e
        .document
        .scene
        .elements
        .iter()
        .find(|el| matches!(el, Element::StickyNote(_)))
        .map(|el| el.id().to_string())
        .expect("the sticky note was created");
    let text_id = e
        .document
        .container_text_id(&note_id)
        .expect("the note carries a bound text");

    e.on_canvas_mouse_down(200.0, 120.0, plain(), 2);
    e.handle_key("h", Some("h".into()), plain(), VP);
    e.handle_key("escape", None, plain(), VP);

    e.on_canvas_mouse_down(180.0, 170.0, plain(), 1);
    e.on_canvas_mouse_up(180.0, 170.0, plain());

    assert!(
        e.document.selected.contains(&note_id),
        "clicking the label band must select the note"
    );
    assert!(
        !e.document.selected.contains(&text_id),
        "the label must never be selected on its own"
    );
}

#[test]
fn resizing_a_sticky_note_tracks_the_pointer_linearly() {
    let mut e = Editor::new();
    e.set_tool(Tool::StickyNote);
    e.on_canvas_mouse_down(100.0, 100.0, plain(), 1);
    e.on_canvas_mouse_up(260.0, 240.0, plain());

    e.set_tool(Tool::Selection);

    e.on_canvas_mouse_down(110.0, 110.0, plain(), 1);
    e.on_canvas_mouse_up(110.0, 110.0, plain());

    e.on_canvas_mouse_down(260.0, 240.0, plain(), 1);
    for i in 1..=8 {
        let d = i as f32 * 20.0;
        e.on_canvas_mouse_move(260.0 + d, 240.0 + d, plain());
        let w = e
            .document
            .scene
            .elements
            .iter()
            .find(|el| matches!(el, Element::StickyNote(_)))
            .unwrap()
            .base()
            .width;
        assert!(
            (w - (160.0 + d as f64)).abs() < 1e-6,
            "step {i}: width {w} should track the pointer, not compound"
        );
    }
    e.on_canvas_mouse_up(420.0, 400.0, plain());
}
