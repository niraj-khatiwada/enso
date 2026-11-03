use gpui_excalidraw::core::{
    factory::{
        ElementOptions, new_arrow, new_diamond, new_ellipse, new_freedraw, new_image, new_line,
        new_rectangle, new_sticky_note, new_text,
    },
    geometry::Point,
};

fn opts() -> ElementOptions {
    ElementOptions::default()
}

#[test]
fn test_new_rectangle() {
    let e = new_rectangle(10.0, 20.0, 100.0, 50.0, &opts());
    assert_eq!(e.base().x, 10.0);
    assert_eq!(e.base().y, 20.0);
    assert_eq!(e.base().width, 100.0);
    assert_eq!(e.base().height, 50.0);
}

#[test]
fn test_new_diamond() {
    let e = new_diamond(0.0, 0.0, 50.0, 50.0, &opts());
    assert!(matches!(e, gpui_excalidraw::core::Element::Diamond { .. }));
}

#[test]
fn test_new_ellipse() {
    let e = new_ellipse(0.0, 0.0, 50.0, 50.0, &opts());
    assert!(matches!(e, gpui_excalidraw::core::Element::Ellipse { .. }));
}

#[test]
fn test_new_line_bounds() {
    let e = new_line(vec![Point::new(0.0, 0.0), Point::new(100.0, 50.0)], &opts());
    assert_eq!(e.base().width, 100.0);
    assert_eq!(e.base().height, 50.0);
}

#[test]
fn test_new_arrow_arrowheads() {
    let e = new_arrow(
        vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
        None,
        Some(gpui_excalidraw::core::Arrowhead::Arrow),
        &opts(),
    );
    if let gpui_excalidraw::core::Element::Arrow(a) = &e {
        assert_eq!(
            a.linear.end_arrowhead,
            Some(gpui_excalidraw::core::Arrowhead::Arrow)
        );
        assert_eq!(a.linear.start_arrowhead, None);
    } else {
        panic!("expected arrow");
    }
}

#[test]
fn test_new_freedraw_points() {
    let e = new_freedraw(
        vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 0.0),
        ],
        &opts(),
    );
    if let gpui_excalidraw::core::Element::Freedraw(f) = &e {
        assert_eq!(f.points.len(), 3);
        assert_eq!(f.pressures.len(), 3);
    } else {
        panic!("expected freedraw");
    }
}

#[test]
fn test_new_text() {
    let e = new_text(0.0, 0.0, "hello", 20.0, &opts());
    if let gpui_excalidraw::core::Element::Text(t) = &e {
        assert_eq!(t.text, "hello");
        assert_eq!(t.font_size, 20.0);
    } else {
        panic!("expected text");
    }
}

#[test]
fn test_unique_ids() {
    let a = new_rectangle(0.0, 0.0, 10.0, 10.0, &opts());
    let b = new_rectangle(0.0, 0.0, 10.0, 10.0, &opts());
    assert_ne!(a.id(), b.id());
}

#[test]
fn test_new_image() {
    let e = new_image(
        10.0,
        20.0,
        200.0,
        150.0,
        Some("file-1".to_string()),
        &opts(),
    );
    if let gpui_excalidraw::core::Element::Image(img) = &e {
        assert_eq!(img.file_id.as_deref(), Some("file-1"));
        assert_eq!(img.base.width, 200.0);
        assert_eq!(img.base.height, 150.0);
        assert_eq!(img.status, gpui_excalidraw::core::ImageStatus::Pending);
    } else {
        panic!("expected image");
    }
}

#[test]
fn test_new_sticky_note() {
    let e = new_sticky_note(0.0, 0.0, 120.0, 120.0, &opts());
    if let gpui_excalidraw::core::Element::StickyNote(s) = &e {
        assert_eq!(s.base_height, 120.0);
        assert_eq!(s.base.width, 120.0);
    } else {
        panic!("expected sticky note");
    }
}
