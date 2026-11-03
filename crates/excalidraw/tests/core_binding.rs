use gpui_excalidraw::core::{
    binding::*, element::Element, factory::*, geometry::Point, scene::Scene,
};

fn opts() -> ElementOptions {
    ElementOptions::default()
}

fn arrow_points(scene: &Scene, id: &str) -> Vec<Point> {
    match scene.get(id).unwrap() {
        Element::Arrow(a) => a.linear.points.clone(),
        _ => panic!("not an arrow"),
    }
}

#[test]
fn focus_point_rectangle_right_edge() {
    let r = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let p = focus_point(&r, Point::new(200.0, 50.0));
    assert!((p.x - 100.0).abs() < 1e-6, "x was {}", p.x);
    assert!((p.y - 50.0).abs() < 1e-6, "y was {}", p.y);
}

#[test]
fn focus_point_ellipse_and_diamond_match_border() {
    let e = new_ellipse(0.0, 0.0, 100.0, 100.0, &opts());
    let pe = focus_point(&e, Point::new(200.0, 50.0));
    assert!((pe.x - 100.0).abs() < 1e-6);

    let d = new_diamond(0.0, 0.0, 100.0, 100.0, &opts());
    let pd = focus_point(&d, Point::new(200.0, 50.0));
    assert!((pd.x - 100.0).abs() < 1e-6);
}

#[test]
fn focus_point_above_lands_on_top_edge() {
    let r = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let p = focus_point(&r, Point::new(50.0, -100.0));
    assert!((p.x - 50.0).abs() < 1e-6);
    assert!((p.y - 0.0).abs() < 1e-6);
}

#[test]
fn find_bindable_detects_shape_and_respects_exclude() {
    let mut scene = Scene::new();
    let rect = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let id = rect.id().to_string();
    scene.add(rect);

    assert_eq!(
        find_bindable(&scene, Point::new(50.0, 50.0), 10.0, &[]).as_deref(),
        Some(id.as_str())
    );
    assert_eq!(
        find_bindable(&scene, Point::new(105.0, 50.0), 10.0, &[]).as_deref(),
        Some(id.as_str())
    );

    assert!(find_bindable(&scene, Point::new(500.0, 500.0), 10.0, &[]).is_none());

    assert!(
        find_bindable(
            &scene,
            Point::new(50.0, 50.0),
            10.0,
            std::slice::from_ref(&id)
        )
        .is_none()
    );
}

#[test]
fn make_binding_normalises_fixed_point() {
    let r = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let b = make_binding(&r, Point::new(200.0, 50.0));
    assert!((0.0..=1.0).contains(&b.fixed_point[0]));
    assert!((0.0..=1.0).contains(&b.fixed_point[1]));
    assert!((b.fixed_point[0] - 1.0).abs() < 1e-6);
    assert_eq!(b.element_id, r.id());
}

#[test]
fn bound_arrow_follows_moved_shape() {
    let mut scene = Scene::new();
    let rect = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let rect_id = rect.id().to_string();
    scene.add(rect);

    let arrow = new_arrow(
        vec![Point::new(200.0, 50.0), Point::new(140.0, 50.0)],
        None,
        Some(gpui_excalidraw::core::Arrowhead::Arrow),
        &opts(),
    );
    let arrow_id = arrow.id().to_string();
    scene.add(arrow);

    let binding = make_binding(scene.get(&rect_id).unwrap(), Point::new(200.0, 50.0));
    if let Some(Element::Arrow(a)) = scene.get_mut(&arrow_id) {
        a.linear.end_binding = Some(binding);
    }
    update_bindings(&mut scene);
    let pts = arrow_points(&scene, &arrow_id);
    assert!((pts[1].x - 100.0).abs() < 1e-6, "end x = {}", pts[1].x);

    gpui_excalidraw::core::transform::translate_element(
        scene.get_mut(&rect_id).unwrap(),
        60.0,
        0.0,
    );
    update_bindings(&mut scene);
    let pts = arrow_points(&scene, &arrow_id);
    assert!((pts[1].x - 160.0).abs() < 1e-6, "end x = {}", pts[1].x);
    assert!((pts[1].y - 50.0).abs() < 1e-6);
}

#[test]
fn binding_survives_rotation() {
    let mut scene = Scene::new();
    let rect = new_rectangle(0.0, 0.0, 100.0, 100.0, &opts());
    let rect_id = rect.id().to_string();
    scene.add(rect);

    let arrow = new_arrow(
        vec![Point::new(200.0, 50.0), Point::new(140.0, 50.0)],
        None,
        None,
        &opts(),
    );
    let arrow_id = arrow.id().to_string();
    scene.add(arrow);
    let binding = make_binding(scene.get(&rect_id).unwrap(), Point::new(200.0, 50.0));
    if let Some(Element::Arrow(a)) = scene.get_mut(&arrow_id) {
        a.linear.end_binding = Some(binding);
    }

    gpui_excalidraw::core::transform::rotate_element(
        scene.get_mut(&rect_id).unwrap(),
        std::f64::consts::FRAC_PI_4,
        Point::new(50.0, 50.0),
    );
    update_bindings(&mut scene);
    let pts = arrow_points(&scene, &arrow_id);

    let b = gpui_excalidraw::core::bounds::element_bounds(scene.get(&rect_id).unwrap());
    assert!(
        b.expand(0.5).contains(pts[1]),
        "endpoint {:?} off shape",
        pts[1]
    );
}

#[test]
fn snap_to_grid_rounds_to_multiple() {
    assert_eq!(snap_to_grid(27.0, 20.0), 20.0);
    assert_eq!(snap_to_grid(33.0, 20.0), 40.0);
    assert_eq!(snap_to_grid(-7.0, 20.0), -0.0);

    assert_eq!(snap_to_grid(27.0, 0.0), 27.0);
}

#[test]
fn constrain_angle_snaps_to_15_degrees() {
    let origin = Point::zero();

    let p = constrain_angle(origin, Point::new(100.0, 10.0), 15.0);
    assert!(p.y.abs() < 1e-6, "y = {}", p.y);

    let p = constrain_angle(origin, Point::new(100.0, 84.0), 15.0);
    assert!((p.x - p.y).abs() < 1e-6, "expected 45deg, got {:?}", p);
}

#[test]
fn constrain_square_forces_equal_extent() {
    let p = constrain_square(Point::zero(), Point::new(100.0, 40.0));
    assert_eq!((p.x, p.y), (100.0, 100.0));
    let p = constrain_square(Point::zero(), Point::new(-30.0, -90.0));
    assert_eq!((p.x, p.y), (-90.0, -90.0));
}

#[test]
fn frames_are_kept_behind_other_elements() {
    let mut scene = Scene::new();
    scene.add(new_rectangle(0.0, 0.0, 10.0, 10.0, &opts()));
    let frame = new_frame(0.0, 0.0, 500.0, 500.0, None, &opts());
    let frame_id = frame.id().to_string();
    scene.add(frame);
    scene.add(new_ellipse(20.0, 20.0, 10.0, 10.0, &opts()));

    assert_eq!(scene.elements[0].id(), frame_id);
}

#[test]
fn sticky_note_is_rounded_and_tinted() {
    let s = new_sticky_note(0.0, 0.0, 100.0, 100.0, &opts());
    let base = s.base();
    assert!(base.roundness.is_some());
    assert_eq!(base.background_color, "#ffec99");
}

use gpui_excalidraw::core::text::wrap_text;

#[test]
fn wrap_text_breaks_cjk_between_characters() {
    let out = wrap_text(&"字".repeat(13), 120.0, 20.0);
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].chars().count(), 6);
    assert_eq!(lines[1].chars().count(), 6);
    assert_eq!(lines[2].chars().count(), 1);
}

#[test]
fn wrap_text_prefers_the_last_space_for_latin() {
    assert_eq!(wrap_text("hello world", 70.0, 20.0), "hello\nworld");

    let out = wrap_text("abcdefghij", 40.0, 20.0);
    assert_eq!(out, "abc\ndef\nghi\nj");
}

#[test]
fn wrap_text_keeps_hard_newlines_and_blank_lines() {
    assert_eq!(wrap_text("a\n\nb", 1000.0, 20.0), "a\n\nb");

    assert_eq!(wrap_text("abc", f64::INFINITY, 20.0), "abc");
}

#[test]
fn sticky_note_wraps_and_grows_to_fit_its_label() {
    let mut scene = Scene::new();
    let (note, text) = new_sticky_note_with_text(0.0, 0.0, 140.0, 120.0, &opts());
    let note_id = note.id().to_string();
    let text_id = text.id().to_string();
    scene.add(note);
    scene.add(text);

    if let Some(Element::Text(t)) = scene.get_mut(&text_id) {
        t.text = "字".repeat(60);
    }
    sync_container_text(&mut scene);

    let note_h = scene.get(&note_id).unwrap().base().height;
    assert!((note_h - 266.0).abs() < 1e-6, "note height {note_h}");
    match scene.get(&text_id).unwrap() {
        Element::Text(t) => {
            assert!(
                (t.base.width - 124.0).abs() < 1e-6,
                "text width {}",
                t.base.width
            );
            assert!(
                (t.base.height - 250.0).abs() < 1e-6,
                "text height {}",
                t.base.height
            );
            assert_eq!(
                wrap_text(&t.text, t.base.width, t.font_size)
                    .split('\n')
                    .count(),
                10
            );
        }
        _ => panic!("not a text element"),
    }
}

#[test]
fn streamline_reduces_jitter_and_keeps_endpoints() {
    let pts: Vec<Point> = (0..20)
        .map(|i| Point::new(i as f64 * 10.0, if i % 2 == 0 { 0.0 } else { 8.0 }))
        .collect();
    let out = streamline_points(&pts, 0.5);
    assert_eq!(out.len(), pts.len());
    assert_eq!(out[0], pts[0]);
    assert_eq!(*out.last().unwrap(), *pts.last().unwrap());

    let amp_raw = (pts[1].y - pts[0].y).abs();
    let amp_smooth = (out[1].y - out[0].y).abs();
    assert!(amp_smooth < amp_raw, "{amp_smooth} !< {amp_raw}");
}
