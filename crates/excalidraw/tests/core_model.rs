use gpui_excalidraw::core::{
    bounds::{Bounds, common_bounds, element_bounds},
    collision::{hit_test_element, hit_test_rectangle},
    element::{Element, ElementBase, ElementType},
    geometry::Point,
    scene::Scene,
    transform::{rotate_element, translate_element},
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = ElementBase::new("test".to_string(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    Element::Rectangle { base }
}

#[test]
fn test_element_bounds_basic() {
    let e = rect(10.0, 20.0, 100.0, 50.0);
    let b = element_bounds(&e);
    assert_eq!(b.min_x, 10.0);
    assert_eq!(b.min_y, 20.0);
    assert_eq!(b.max_x, 110.0);
    assert_eq!(b.max_y, 70.0);
    assert_eq!(b.width(), 100.0);
    assert_eq!(b.height(), 50.0);
}

#[test]
fn test_element_bounds_rotated() {
    let mut e = rect(0.0, 0.0, 100.0, 100.0);
    rotate_element(&mut e, std::f64::consts::FRAC_PI_4, Point::new(50.0, 50.0));
    let b = element_bounds(&e);
    assert!(b.width() > 100.0);
    assert!(b.height() > 100.0);
}

#[test]
fn test_hit_test_rectangle_inside() {
    let e = rect(0.0, 0.0, 100.0, 100.0);
    assert!(hit_test_rectangle(&e, Point::new(50.0, 50.0)));
    assert!(!hit_test_rectangle(&e, Point::new(150.0, 50.0)));
}

#[test]
fn test_hit_test_element() {
    let e = rect(0.0, 0.0, 100.0, 100.0);
    assert!(hit_test_element(&e, Point::new(50.0, 50.0), 5.0));
    assert!(!hit_test_element(&e, Point::new(200.0, 200.0), 5.0));
}

#[test]
fn test_translate_element() {
    let mut e = rect(0.0, 0.0, 100.0, 100.0);
    translate_element(&mut e, 10.0, 20.0);
    assert_eq!(e.base().x, 10.0);
    assert_eq!(e.base().y, 20.0);
}

#[test]
fn test_rotate_element() {
    let mut e = rect(0.0, 0.0, 100.0, 100.0);
    rotate_element(&mut e, std::f64::consts::PI / 2.0, Point::new(50.0, 50.0));
    assert!((e.base().angle - std::f64::consts::PI / 2.0).abs() < 1e-6);
}

#[test]
fn test_scene_add_remove() {
    let mut scene = Scene::new();
    assert!(scene.is_empty());
    let e = rect(0.0, 0.0, 10.0, 10.0);
    scene.add(e.clone());
    assert_eq!(scene.len(), 1);
    assert!(scene.get("test").is_some());
    let removed = scene.remove("test");
    assert!(removed.is_some());
    assert!(scene.is_empty());
}

#[test]
fn test_common_bounds() {
    let a = rect(0.0, 0.0, 100.0, 100.0);
    let b = rect(100.0, 100.0, 100.0, 100.0);
    let bounds = common_bounds(&[&a, &b]).unwrap();
    assert_eq!(bounds.min_x, 0.0);
    assert_eq!(bounds.min_y, 0.0);
    assert_eq!(bounds.max_x, 200.0);
    assert_eq!(bounds.max_y, 200.0);
}

#[test]
fn test_bounds_intersects() {
    let a = Bounds::new(0.0, 0.0, 100.0, 100.0);
    let b = Bounds::new(50.0, 50.0, 150.0, 150.0);
    let c = Bounds::new(200.0, 200.0, 300.0, 300.0);
    assert!(a.intersects(&b));
    assert!(!a.intersects(&c));
}

#[test]
fn test_serde_roundtrip() {
    let e = rect(10.0, 20.0, 100.0, 50.0);
    let json = serde_json::to_string(&e).unwrap();
    let back: Element = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
}

#[test]
fn test_serde_element_type_tag() {
    let e = rect(0.0, 0.0, 10.0, 10.0);
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"type\":\"rectangle\""));
}

#[test]
fn test_serde_roundtrip_newtype_variants() {
    let opts = gpui_excalidraw::core::factory::ElementOptions::default();
    let cases = vec![
        (
            "text",
            gpui_excalidraw::core::factory::new_text(0.0, 0.0, "hi", 20.0, &opts),
        ),
        (
            "image",
            gpui_excalidraw::core::factory::new_image(
                0.0,
                0.0,
                10.0,
                10.0,
                Some("f".to_string()),
                &opts,
            ),
        ),
        (
            "frame",
            gpui_excalidraw::core::factory::new_frame(0.0, 0.0, 10.0, 10.0, None, &opts),
        ),
        (
            "line",
            gpui_excalidraw::core::factory::new_line(
                vec![Point::new(0.0, 0.0), Point::new(5.0, 5.0)],
                &opts,
            ),
        ),
        (
            "arrow",
            gpui_excalidraw::core::factory::new_arrow(
                vec![Point::new(0.0, 0.0), Point::new(5.0, 5.0)],
                None,
                None,
                &opts,
            ),
        ),
        (
            "freedraw",
            gpui_excalidraw::core::factory::new_freedraw(
                vec![Point::new(0.0, 0.0), Point::new(5.0, 5.0)],
                &opts,
            ),
        ),
        (
            "embeddable",
            gpui_excalidraw::core::factory::new_embeddable(0.0, 0.0, 10.0, 10.0, &opts),
        ),
        (
            "stickynote",
            gpui_excalidraw::core::factory::new_sticky_note(0.0, 0.0, 10.0, 10.0, &opts),
        ),
    ];
    for (tag, element) in cases {
        let json = serde_json::to_string(&element).unwrap();
        assert!(
            json.contains(&format!("\"type\":\"{tag}\"")),
            "expected type tag {tag} in {json}"
        );
        assert!(!json.contains("\"kind\""), "kind must not leak into {json}");
        let back: Element = serde_json::from_str(&json).unwrap();
        assert_eq!(element, back, "roundtrip failed for {tag}");
    }
}
