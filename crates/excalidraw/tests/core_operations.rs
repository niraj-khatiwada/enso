use gpui_excalidraw::core::{
    element::{ConvertTarget, Element, ElementBase, ElementType, GENERIC_TARGETS, LINEAR_TARGETS},
    factory::{ElementOptions, new_arrow, new_line},
    geometry::Point,
    operations::{
        Align, Distribute, align_elements, distribute_elements, group_ids, select_group,
        toggle_element_locked, ungroup_ids,
    },
    scene::Scene,
};

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element {
    let mut base = ElementBase::new(id.to_string(), ElementType::Rectangle);
    base.x = x;
    base.y = y;
    base.width = w;
    base.height = h;
    Element::Rectangle { base }
}

fn scene_with_two() -> Scene {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 100.0, 100.0));
    scene.add(rect("b", 200.0, 0.0, 100.0, 100.0));
    scene
}

#[test]
fn test_toggle_lock() {
    let mut scene = scene_with_two();
    assert!(!scene.get("a").unwrap().base().locked);
    let locked = toggle_element_locked(&mut scene, "a");
    assert!(locked);
    assert!(scene.get("a").unwrap().base().locked);
    let locked = toggle_element_locked(&mut scene, "a");
    assert!(!locked);
}

#[test]
fn test_group_and_select_group() {
    let mut scene = scene_with_two();
    let gid = group_ids(&mut scene, &["a".to_string(), "b".to_string()]);
    assert!(gid.is_some());
    assert_eq!(scene.get("a").unwrap().base().group_ids.len(), 1);
    let members = select_group(&scene, "a");
    assert_eq!(members.len(), 2);
}

#[test]
fn test_ungroup() {
    let mut scene = scene_with_two();
    group_ids(&mut scene, &["a".to_string(), "b".to_string()]);
    ungroup_ids(&mut scene, &["a".to_string(), "b".to_string()]);
    assert!(scene.get("a").unwrap().base().group_ids.is_empty());
}

#[test]
fn test_align_left() {
    let mut scene = scene_with_two();
    align_elements(&mut scene, &["a".to_string(), "b".to_string()], Align::Left);
    assert_eq!(scene.get("a").unwrap().base().x, 0.0);
    assert_eq!(scene.get("b").unwrap().base().x, 0.0);
}

#[test]
fn test_align_center_horizontal() {
    let mut scene = scene_with_two();
    align_elements(
        &mut scene,
        &["a".to_string(), "b".to_string()],
        Align::CenterH,
    );
    let a_cx = scene.get("a").unwrap().base().x + scene.get("a").unwrap().base().width / 2.0;
    let b_cx = scene.get("b").unwrap().base().x + scene.get("b").unwrap().base().width / 2.0;
    assert!((a_cx - b_cx).abs() < 1e-6);
}

#[test]
fn test_align_top() {
    let mut scene = scene_with_two();
    align_elements(&mut scene, &["a".to_string(), "b".to_string()], Align::Top);
    assert_eq!(scene.get("a").unwrap().base().y, 0.0);
    assert_eq!(scene.get("b").unwrap().base().y, 0.0);
}

#[test]
fn test_distribute_horizontal_three() {
    let mut scene = Scene::new();
    scene.add(rect("a", 0.0, 0.0, 10.0, 10.0));
    scene.add(rect("b", 10.0, 0.0, 10.0, 10.0));
    scene.add(rect("c", 100.0, 0.0, 10.0, 10.0));
    distribute_elements(
        &mut scene,
        &["a".to_string(), "b".to_string(), "c".to_string()],
        Distribute::Horizontal,
    );
    let a_cx = scene.get("a").unwrap().base().x + 5.0;
    let b_cx = scene.get("b").unwrap().base().x + 5.0;
    let c_cx = scene.get("c").unwrap().base().x + 5.0;
    assert!((b_cx - a_cx - (c_cx - a_cx) / 2.0).abs() < 1e-6);
}

#[test]
fn test_group_single_returns_none() {
    let mut scene = scene_with_two();
    assert!(group_ids(&mut scene, &["a".to_string()]).is_none());
}

#[test]
fn test_select_group_non_grouped_returns_self() {
    let scene = scene_with_two();
    let members = select_group(&scene, "a");
    assert_eq!(members, vec!["a".to_string()]);
}

fn line() -> Element {
    new_line(
        vec![Point::new(0.0, 0.0), Point::new(50.0, 40.0)],
        &ElementOptions::default(),
    )
}

#[test]
fn generic_shapes_convert_into_each_other_and_keep_their_geometry() {
    let mut rectangle = rect("a", 10.0, 20.0, 30.0, 40.0);
    rectangle.base_mut().stroke_color = "#ff0000".to_string();
    let version = rectangle.base().version;

    let diamond = rectangle
        .convert_to(ConvertTarget::Diamond)
        .expect("rectangle to diamond");
    assert_eq!(diamond.kind(), ElementType::Diamond);
    assert_eq!(diamond.id(), "a");
    assert_eq!(diamond.base().x, 10.0);
    assert_eq!(diamond.base().y, 20.0);
    assert_eq!(diamond.base().width, 30.0);
    assert_eq!(diamond.base().height, 40.0);
    assert_eq!(diamond.base().stroke_color, "#ff0000");
    assert!(
        diamond.base().version > version,
        "a conversion is a new version"
    );

    let ellipse = diamond
        .convert_to(ConvertTarget::Ellipse)
        .expect("diamond to ellipse");
    assert_eq!(ellipse.kind(), ElementType::Ellipse);
    let back = ellipse
        .convert_to(ConvertTarget::Rectangle)
        .expect("ellipse back to rectangle");
    assert_eq!(back.kind(), ElementType::Rectangle);
}

#[test]
fn a_generic_shape_offers_only_the_three_generic_types() {
    let rectangle = rect("a", 0.0, 0.0, 10.0, 10.0);
    assert_eq!(rectangle.convertible_targets(), &GENERIC_TARGETS);
    assert_eq!(rectangle.convert_target(), Some(ConvertTarget::Rectangle));
    assert!(
        rectangle.convert_to(ConvertTarget::Line).is_none(),
        "a rectangle must not turn into a line"
    );
}

#[test]
fn converting_to_the_type_it_already_has_is_a_no_op() {
    let rectangle = rect("a", 0.0, 0.0, 10.0, 10.0);
    assert!(rectangle.convert_to(ConvertTarget::Rectangle).is_none());
}

#[test]
fn linear_elements_switch_between_their_four_subtypes() {
    let line = line();
    assert_eq!(line.convertible_targets(), &LINEAR_TARGETS);
    assert_eq!(line.convert_target(), Some(ConvertTarget::Line));

    let sharp = line
        .convert_to(ConvertTarget::SharpArrow)
        .expect("line to sharp arrow");
    assert_eq!(sharp.kind(), ElementType::Arrow);
    assert_eq!(sharp.convert_target(), Some(ConvertTarget::SharpArrow));

    let curved = sharp
        .convert_to(ConvertTarget::CurvedArrow)
        .expect("sharp to curved arrow");
    assert_eq!(curved.convert_target(), Some(ConvertTarget::CurvedArrow));

    let elbow = curved
        .convert_to(ConvertTarget::ElbowArrow)
        .expect("curved to elbow arrow");
    assert_eq!(elbow.convert_target(), Some(ConvertTarget::ElbowArrow));

    let Element::Arrow(arrow) = &elbow else {
        panic!("an elbow arrow is still an arrow");
    };
    assert!(arrow.elbowed);
    assert_eq!(
        arrow.linear.points.len(),
        2,
        "the route survives the switch"
    );
}

#[test]
fn an_arrow_bound_to_a_shape_is_not_convertible() {
    let mut arrow = new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(50.0, 40.0)],
        None,
        None,
        &ElementOptions::default(),
    );
    assert_eq!(arrow.convertible_targets(), &LINEAR_TARGETS);

    let Element::Arrow(a) = &mut arrow else {
        unreachable!()
    };
    a.linear.end_binding = Some(gpui_excalidraw::core::element::FixedPointBinding {
        element_id: "target".to_string(),
        fixed_point: [0.5, 0.5],
        mode: gpui_excalidraw::core::element::BindMode::Inside,
    });
    assert!(arrow.convertible_targets().is_empty());
    assert!(arrow.convert_to(ConvertTarget::SharpArrow).is_none());
}

#[test]
fn a_bound_line_still_converts() {
    let line = line();
    let Element::Line(mut l) = line.clone() else {
        unreachable!()
    };
    l.linear.end_binding = Some(gpui_excalidraw::core::element::FixedPointBinding {
        element_id: "target".to_string(),
        fixed_point: [0.5, 0.5],
        mode: gpui_excalidraw::core::element::BindMode::Inside,
    });
    let bound = Element::Line(l);
    assert_eq!(bound.convertible_targets(), &LINEAR_TARGETS);
}

#[test]
fn new_generic_elements_start_as_rectangles() {
    let fresh = rect("a", 0.0, 0.0, 5.0, 5.0);
    assert_eq!(fresh.convert_target(), Some(ConvertTarget::Rectangle));
    assert_eq!(
        fresh.convertible_targets().len(),
        GENERIC_TARGETS.len(),
        "the popup offers one button per generic type"
    );
}
