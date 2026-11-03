use gpui_excalidraw::{
    core::{
        element::{Element, ElementBase, ElementType},
        geometry::Point,
        scene::Scene,
    },
    ui::{
        i18n::{I18n, Language},
        state::{Document, History, Tool, screen_to_world, world_to_screen},
    },
};

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = ElementBase::new(id.to_string(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    Element::Rectangle { base }
}

#[test]
fn test_document_default_tool() {
    let doc = Document::new();
    assert_eq!(doc.tool, Tool::Selection);
    assert_eq!(doc.zoom, 1.0);
    assert!(doc.scene.is_empty());
}

#[test]
fn test_tool_to_element_type() {
    assert_eq!(
        Tool::Rectangle.to_element_type(),
        Some(ElementType::Rectangle)
    );
    assert_eq!(Tool::Ellipse.to_element_type(), Some(ElementType::Ellipse));
    assert_eq!(Tool::Selection.to_element_type(), None);
    assert_eq!(Tool::Hand.to_element_type(), None);
}

#[test]
fn test_selection() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    doc.select("a");
    assert_eq!(doc.selected.len(), 1);
    assert_eq!(doc.selected_elements().len(), 1);
    doc.clear_selection();
    assert!(doc.selected.is_empty());
}

#[test]
fn test_element_at_hit() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    let hit = doc.element_at(Point::new(50.0, 50.0), 5.0);
    assert_eq!(hit.as_deref(), Some("a"));
    let miss = doc.element_at(Point::new(500.0, 500.0), 5.0);
    assert!(miss.is_none());
}

#[test]
fn test_container_label_is_not_selectable_on_its_own() {
    use gpui_excalidraw::core::factory::{ElementOptions, new_sticky_note_with_text};

    let mut doc = Document::new();
    let (note, mut label) =
        new_sticky_note_with_text(0.0, 0.0, 200.0, 160.0, &ElementOptions::default());
    let note_id = note.id().to_string();
    let label_id = label.id().to_string();

    if let Element::Text(t) = &mut label {
        t.text = "hello".to_string();
        t.base.x = 70.0;
        t.base.y = 68.0;
        t.base.width = 60.0;
        t.base.height = 24.0;
    }
    doc.scene.add(note);
    doc.scene.add(label);

    let point = Point::new(80.0, 78.0);
    assert_eq!(
        doc.element_at(point, 5.0).as_deref(),
        Some(note_id.as_str()),
        "a click on the label must resolve to the note"
    );

    assert_eq!(
        doc.text_target_at(point, 5.0).as_deref(),
        Some(label_id.as_str())
    );

    doc.select_all();
    assert!(doc.selected.contains(&note_id));
    assert!(!doc.selected.contains(&label_id));
}

#[test]
fn test_duplicate_and_paste_carry_the_container_label() {
    use gpui_excalidraw::core::factory::{ElementOptions, new_sticky_note_with_text};

    let mut doc = Document::new();
    let (note, label) =
        new_sticky_note_with_text(0.0, 0.0, 200.0, 160.0, &ElementOptions::default());
    let note_id = note.id().to_string();
    let label_id = label.id().to_string();
    doc.scene.add(note);
    doc.scene.add(label);

    doc.select(&note_id);
    doc.duplicate_selected();
    assert_eq!(doc.selected.len(), 1, "only the note joins the selection");
    let copy_id = doc.selected.iter().next().unwrap().clone();
    let copy_label = doc
        .container_text_id(&copy_id)
        .expect("the duplicated note carries its own label");
    assert_ne!(copy_label, label_id, "the label must be a copy as well");

    assert_eq!(
        doc.container_text_id(&note_id).as_deref(),
        Some(label_id.as_str())
    );
    assert_eq!(
        doc.container_text_id(&copy_id).as_deref(),
        Some(copy_label.as_str())
    );

    doc.selected.clear();
    doc.select(&note_id);
    doc.copy_selected();
    doc.paste(20.0, 20.0);
    let pasted_id = doc.selected.iter().next().unwrap().clone();
    let pasted_label = doc
        .container_text_id(&pasted_id)
        .expect("the pasted note carries a label");
    assert_ne!(pasted_label, label_id);
}

#[test]
fn test_move_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.move_selected(5.0, 10.0);
    let e = doc.scene.get("a").unwrap();
    assert_eq!(e.base().x, 5.0);
    assert_eq!(e.base().y, 10.0);
}

#[test]
fn test_history_undo_redo() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    let mut history = History::new(100);

    history.push(&scene);
    scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    assert_eq!(scene.len(), 2);

    assert!(history.undo(&mut scene));
    assert_eq!(scene.len(), 1);

    assert!(history.redo(&mut scene));
    assert_eq!(scene.len(), 2);
}

#[test]
fn test_history_empty() {
    let mut scene = Scene::new();
    let mut history = History::new(100);
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert!(!history.undo(&mut scene));
}

#[test]
fn test_screen_world_roundtrip() {
    let world = Point::new(100.0, 200.0);
    let zoom = 2.0;
    let scroll = Point::new(50.0, 60.0);
    let screen = world_to_screen(world, zoom, scroll);
    let back = screen_to_world(screen, zoom, scroll);
    assert!((back.x - world.x).abs() < 1e-9);
    assert!((back.y - world.y).abs() < 1e-9);
}

#[test]
fn test_i18n_translations() {
    let i18n = I18n::new(Language::En);
    assert_eq!(i18n.t("toolbar.rectangle"), "Rectangle");

    let i18n = I18n::new(Language::ZhCn);
    assert_eq!(i18n.t("toolbar.rectangle"), "矩形");

    let i18n = I18n::new(Language::Ja);
    assert_eq!(i18n.t("toolbar.rectangle"), "長方形");
}

#[test]
fn test_i18n_language_switch() {
    let i18n = I18n::new(Language::En);
    assert_eq!(i18n.t("action.undo"), "Undo");
    i18n.set_language(Language::ZhCn);
    assert_eq!(i18n.t("action.undo"), "撤销");
}

#[test]
fn test_i18n_missing_key_fallback() {
    let i18n = I18n::new(Language::En);
    assert_eq!(i18n.t("nonexistent.key"), "nonexistent.key");
}

#[test]
fn test_language_code_roundtrip() {
    assert_eq!(Language::from_code("zh-CN"), Language::ZhCn);
    assert_eq!(Language::from_code("ja"), Language::Ja);
    assert_eq!(Language::from_code("unknown"), Language::En);
}

#[test]
fn test_resize_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.select("a");
    doc.resize_selected(2.0, 2.0, Point::new(0.0, 0.0));
    let e = doc.scene.get("a").unwrap();
    assert_eq!(e.base().width, 200.0);
    assert_eq!(e.base().height, 200.0);
}

#[test]
fn test_rotate_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.select("a");
    doc.rotate_selected(std::f64::consts::FRAC_PI_2, Point::new(50.0, 50.0));
    let e = doc.scene.get("a").unwrap();
    assert!((e.base().angle - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn test_duplicate_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.select("a");
    doc.duplicate_selected();
    assert_eq!(doc.scene.len(), 2);
    assert_eq!(doc.selected.len(), 1);
}

#[test]
fn test_group_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    doc.select_many(vec!["a".to_string(), "b".to_string()]);
    doc.group_selected();
    assert_eq!(doc.scene.get("a").unwrap().base().group_ids.len(), 1);
    assert_eq!(doc.scene.get("b").unwrap().base().group_ids.len(), 1);
}

#[test]
fn test_set_selected_stroke_color() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.set_selected_stroke_color("#ff0000");
    assert_eq!(doc.scene.get("a").unwrap().base().stroke_color, "#ff0000");
}

#[test]
fn test_delete_selected() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.delete_selected();
    assert!(doc.scene.is_empty());
    assert!(doc.selected.is_empty());
}

#[test]
fn test_set_selected_locked() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.set_selected_locked(true);
    assert!(doc.scene.get("a").unwrap().base().locked);
}

#[test]
fn test_set_selected_text() {
    let mut doc = Document::new();
    doc.scene.add(gpui_excalidraw::core::factory::new_text(
        0.0,
        0.0,
        "abc",
        20.0,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    ));
    let id = doc.scene.elements[0].id().to_string();
    doc.select(&id);
    doc.set_selected_text("xyz");
    if let Element::Text(t) = doc.scene.get(&id).unwrap() {
        assert_eq!(t.text, "xyz");
    } else {
        panic!("expected text");
    }
}

#[test]
fn test_selection_bounds_multi() {
    use gpui_excalidraw::ui::state::selection_bounds;
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.scene.add(rect("b", 100.0, 100.0, 100.0, 100.0));
    doc.select_many(vec!["a".to_string(), "b".to_string()]);
    let b = selection_bounds(&doc.scene, &doc.selected).unwrap();
    assert_eq!(b.min_x, 0.0);
    assert_eq!(b.min_y, 0.0);
    assert_eq!(b.max_x, 200.0);
    assert_eq!(b.max_y, 200.0);
}

#[test]
fn test_set_selected_roundness() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.set_selected_roundness(Some(gpui_excalidraw::core::types::Roundness {
        kind: gpui_excalidraw::core::types::RoundnessType::ProportionalRadius,
        value: Some(0.25),
    }));
    let e = doc.scene.get("a").unwrap();
    assert!(e.base().roundness.is_some());
    assert_eq!(e.base().roundness.unwrap().value, Some(0.25));
}

#[test]
fn test_set_selected_font_family() {
    let mut doc = Document::new();
    doc.scene.add(gpui_excalidraw::core::factory::new_text(
        0.0,
        0.0,
        "hi",
        20.0,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    ));
    let id = doc.scene.elements[0].id().to_string();
    doc.select(&id);
    doc.set_selected_font_family(gpui_excalidraw::core::types::FontFamily::Helvetica);
    if let Element::Text(t) = doc.scene.get(&id).unwrap() {
        assert_eq!(
            t.font_family,
            gpui_excalidraw::core::types::FontFamily::Helvetica
        );
    } else {
        panic!("expected text");
    }
}

#[test]
fn test_set_selected_text_align() {
    let mut doc = Document::new();
    doc.scene.add(gpui_excalidraw::core::factory::new_text(
        0.0,
        0.0,
        "hi",
        20.0,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    ));
    let id = doc.scene.elements[0].id().to_string();
    doc.select(&id);
    doc.set_selected_text_align(gpui_excalidraw::core::types::TextAlign::Center);
    if let Element::Text(t) = doc.scene.get(&id).unwrap() {
        assert_eq!(
            t.text_align,
            gpui_excalidraw::core::types::TextAlign::Center
        );
    } else {
        panic!("expected text");
    }
}

#[test]
fn test_library_add_and_insert() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.add_selected_to_library();
    assert_eq!(doc.library.len(), 1);

    let lib_id = doc.library[0].id().to_string();
    let new_id = doc.insert_from_library(&lib_id);
    assert!(new_id.is_some());
    assert_eq!(doc.scene.len(), 2);
}

#[test]
fn test_library_remove() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.add_selected_to_library();
    let lib_id = doc.library[0].id().to_string();
    doc.remove_from_library(&lib_id);
    assert!(doc.library.is_empty());
}

#[test]
fn test_layer_reorder() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    assert_eq!(doc.layer_order(), vec!["a".to_string(), "b".to_string()]);
    doc.select("a");
    doc.bring_forward();
    assert_eq!(doc.layer_order(), vec!["b".to_string(), "a".to_string()]);
}

#[test]
fn test_bring_to_front_and_back() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    doc.scene.add(rect("c", 40.0, 40.0, 10.0, 10.0));
    doc.select("a");
    doc.bring_to_front();
    assert_eq!(
        doc.layer_order(),
        vec!["b".to_string(), "c".to_string(), "a".to_string()]
    );
    doc.select("c");
    doc.send_to_back();
    assert_eq!(
        doc.layer_order(),
        vec!["c".to_string(), "b".to_string(), "a".to_string()]
    );
}

#[test]
fn test_select_all_skips_locked() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.scene.add(rect("b", 20.0, 20.0, 10.0, 10.0));
    doc.select("b");
    doc.set_selected_locked(true);
    doc.select_all();
    assert_eq!(doc.selected.len(), 1);
    assert!(doc.selected.contains("a"));
}

#[test]
fn test_clipboard_copy_and_paste() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.select("a");
    assert!(!doc.has_clipboard());
    doc.copy_selected();
    assert!(doc.has_clipboard());
    assert_eq!(doc.scene.len(), 1);

    doc.paste(20.0, 20.0);
    assert_eq!(doc.scene.len(), 2);

    let new_id = doc.selected.iter().next().unwrap().clone();
    assert_ne!(new_id, "a");
    let e = doc.scene.get(&new_id).unwrap();
    assert_eq!(e.base().x, 20.0);
    assert_eq!(e.base().y, 20.0);

    doc.paste(20.0, 20.0);
    assert_eq!(doc.scene.len(), 3);
}

#[test]
fn test_clipboard_cut_removes_source() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    doc.select("a");
    doc.cut_selected();
    assert!(doc.scene.is_empty());
    assert!(doc.has_clipboard());
}

#[test]
fn test_zoom_to_fit_frames_everything() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    doc.scene.add(rect("b", 900.0, 900.0, 100.0, 100.0));
    doc.zoom_to_fit((800.0, 600.0), 40.0);

    let content_w = 1000.0 * doc.zoom;
    let content_h = 1000.0 * doc.zoom;
    assert!(content_w <= 800.0, "width {content_w}");
    assert!(content_h <= 600.0, "height {content_h}");

    let screen =
        gpui_excalidraw::ui::state::world_to_screen(Point::new(0.0, 0.0), doc.zoom, doc.scroll);
    assert!(screen.x >= -1.0 && screen.y >= -1.0);
}

#[test]
fn test_zoom_to_selection_narrows_on_selection() {
    let mut doc = Document::new();
    doc.scene.add(rect("a", 0.0, 0.0, 50.0, 50.0));
    doc.scene.add(rect("b", 2000.0, 2000.0, 50.0, 50.0));
    doc.select("a");
    doc.zoom_to_selection((800.0, 600.0), 40.0);

    assert!(doc.zoom > 5.0, "zoom was {}", doc.zoom);
}

#[test]
fn test_snap_when_enabled() {
    let mut doc = Document::new();
    doc.grid_size = 20.0;
    assert_eq!(doc.snap(Point::new(27.0, 33.0)), Point::new(27.0, 33.0));
    doc.snap_enabled = true;
    assert_eq!(doc.snap(Point::new(27.0, 33.0)), Point::new(20.0, 40.0));
}

#[test]
fn test_move_linear_endpoint_attaches_binding() {
    let mut doc = Document::new();
    doc.scene.add(rect("r", 0.0, 0.0, 100.0, 100.0));
    let arrow = gpui_excalidraw::core::factory::new_arrow(
        vec![Point::new(200.0, 50.0), Point::new(150.0, 50.0)],
        None,
        Some(gpui_excalidraw::core::Arrowhead::Arrow),
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let aid = arrow.id().to_string();
    doc.scene.add(arrow);

    doc.move_linear_endpoint(&aid, 1, Point::new(95.0, 50.0));
    if let Element::Arrow(a) = doc.scene.get(&aid).unwrap() {
        assert!(a.linear.end_binding.is_some(), "expected a binding");
        assert_eq!(a.linear.end_binding.as_ref().unwrap().element_id, "r");
    } else {
        panic!("expected arrow");
    }

    doc.move_linear_endpoint(&aid, 1, Point::new(600.0, 600.0));
    if let Element::Arrow(a) = doc.scene.get(&aid).unwrap() {
        assert!(a.linear.end_binding.is_none(), "expected detachment");
    }
}

#[test]
fn test_set_selected_arrowhead() {
    let mut doc = Document::new();
    let arrow = gpui_excalidraw::core::factory::new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0)],
        None,
        None,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let aid = arrow.id().to_string();
    doc.scene.add(arrow);
    doc.select(&aid);
    doc.set_selected_arrowhead(true, Some(gpui_excalidraw::core::Arrowhead::Triangle));
    doc.set_selected_arrowhead(false, Some(gpui_excalidraw::core::Arrowhead::Dot));
    if let Element::Arrow(a) = doc.scene.get(&aid).unwrap() {
        assert_eq!(
            a.linear.end_arrowhead,
            Some(gpui_excalidraw::core::Arrowhead::Triangle)
        );
        assert_eq!(
            a.linear.start_arrowhead,
            Some(gpui_excalidraw::core::Arrowhead::Dot)
        );
    } else {
        panic!("expected arrow");
    }
}

#[test]
fn test_selected_linear_points_only_for_single_line() {
    let mut doc = Document::new();
    let arrow = gpui_excalidraw::core::factory::new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0)],
        None,
        None,
        &gpui_excalidraw::core::factory::ElementOptions::default(),
    );
    let aid = arrow.id().to_string();
    doc.scene.add(arrow);
    doc.scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));

    doc.select(&aid);
    assert!(doc.selected_linear_points().is_some());

    doc.select_many(vec![aid.clone(), "a".to_string()]);
    assert!(doc.selected_linear_points().is_none());
}
