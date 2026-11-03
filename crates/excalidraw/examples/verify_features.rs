use gpui_excalidraw::{
    core::{
        Arrowhead, Element, ElementOptions, FillStyle, FontFamily, Point, Roundness, RoundnessType,
        Scene, StrokeStyle,
        binding::{constrain_angle, focus_point, snap_to_grid, update_bindings},
        new_arrow, new_diamond, new_frame, new_freedraw, new_line, new_rectangle, new_sticky_note,
        new_text, streamline_points,
    },
    export_svg, scene_to_json,
};

fn check(label: &str, ok: bool) {
    println!("  [{}] {}", if ok { "PASS" } else { "FAIL" }, label);
    assert!(ok, "verification failed: {label}");
}

fn main() {
    let mut scene = Scene::new();

    let frame = new_frame(
        40.0,
        40.0,
        640.0,
        420.0,
        Some("Flow".to_string()),
        &ElementOptions {
            stroke_color: "#adb5bd".to_string(),
            ..Default::default()
        },
    );
    let frame_id = frame.id().to_string();
    scene.add(frame);

    let mut rect = new_rectangle(
        90.0,
        120.0,
        180.0,
        110.0,
        &ElementOptions {
            stroke_color: "#1971c2".to_string(),
            background_color: "#a5d8ff".to_string(),
            fill_style: FillStyle::Solid,
            ..Default::default()
        },
    );
    if let Element::Rectangle { base } = &mut rect {
        base.roundness = Some(Roundness {
            kind: RoundnessType::ProportionalRadius,
            value: Some(0.25),
        });
    }
    let rect_id = rect.id().to_string();
    scene.add(rect);

    let arrow = new_arrow(
        vec![Point::new(620.0, 300.0), Point::new(430.0, 300.0)],
        None,
        Some(Arrowhead::Arrow),
        &ElementOptions {
            stroke_color: "#e8590c".to_string(),
            ..Default::default()
        },
    );
    let arrow_id = arrow.id().to_string();
    scene.add(arrow);
    {
        let target = scene.get(&rect_id).unwrap().clone();
        let binding =
            gpui_excalidraw::core::binding::make_binding(&target, Point::new(620.0, 300.0));
        if let Some(Element::Arrow(a)) = scene.get_mut(&arrow_id) {
            a.linear.end_binding = Some(binding);
        }
    }
    update_bindings(&mut scene);
    let end_before = match scene.get(&arrow_id).unwrap() {
        Element::Arrow(a) => a.linear.points[1],
        _ => unreachable!(),
    };

    gpui_excalidraw::core::transform::translate_element(
        scene.get_mut(&rect_id).unwrap(),
        120.0,
        60.0,
    );
    update_bindings(&mut scene);
    let end_after = match scene.get(&arrow_id).unwrap() {
        Element::Arrow(a) => a.linear.points[1],
        _ => unreachable!(),
    };

    scene.add(new_diamond(
        470.0,
        120.0,
        140.0,
        110.0,
        &ElementOptions {
            stroke_color: "#2f9e44".to_string(),
            background_color: "#b2f2bb".to_string(),
            fill_style: FillStyle::Hachure,
            ..Default::default()
        },
    ));

    let mut dashed = new_line(
        vec![Point::new(90.0, 300.0), Point::new(330.0, 300.0)],
        &ElementOptions {
            stroke_color: "#6741d9".to_string(),
            stroke_style: StrokeStyle::Dashed,
            stroke_width: 2.0,
            ..Default::default()
        },
    );
    if let Element::Line(l) = &mut dashed {
        l.linear.start_arrowhead = Some(Arrowhead::Dot);
        l.linear.end_arrowhead = Some(Arrowhead::Triangle);
    }
    scene.add(dashed);

    let raw: Vec<Point> = (0..40)
        .map(|i| {
            let x = 400.0 + i as f64 * 5.0;
            let y = 330.0 + (i as f64 * 0.6).sin() * 26.0;
            Point::new(x, y)
        })
        .collect();
    let smoothed = streamline_points(&raw, 0.4);
    scene.add(new_freedraw(
        smoothed.clone(),
        &ElementOptions {
            stroke_color: "#e03131".to_string(),
            ..Default::default()
        },
    ));

    scene.add(new_sticky_note(
        90.0,
        380.0,
        150.0,
        130.0,
        &ElementOptions::default(),
    ));
    let mut title = new_text(
        470.0,
        260.0,
        "gpui-excalidraw",
        28.0,
        &ElementOptions {
            stroke_color: "#1e1e1e".to_string(),
            ..Default::default()
        },
    );
    if let Element::Text(t) = &mut title {
        t.font_family = FontFamily::Helvetica;
    }
    scene.add(title);

    println!("gpui-excalidraw feature verification\n");

    println!("element ordering");
    check(
        "frame is stored first (renders behind)",
        scene.elements[0].id() == frame_id,
    );
    check("scene has 8 elements", scene.len() == 8);

    println!("arrow binding");
    check(
        "bound endpoint snapped onto the rectangle's right edge",
        (end_before.x - 270.0).abs() < 1e-6 && end_before.y > 120.0 && end_before.y < 230.0,
    );
    check(
        "endpoint followed the rectangle when it moved",
        (end_after.x - 390.0).abs() < 1e-6 && end_after.y > 180.0 && end_after.y < 290.0,
    );
    let outline = focus_point(scene.get(&rect_id).unwrap(), Point::new(600.0, 300.0));
    let b = gpui_excalidraw::core::bounds::element_bounds(scene.get(&rect_id).unwrap());
    check(
        "focus point lies on the shape outline",
        b.expand(0.5).contains(outline),
    );

    println!("snapping & constraints");
    check(
        "snap_to_grid(27, 20) == 20",
        snap_to_grid(27.0, 20.0) == 20.0,
    );
    let snapped = constrain_angle(Point::new(0.0, 0.0), Point::new(100.0, 12.0), 15.0);
    check(
        "shift-constrained line is horizontal",
        snapped.y.abs() < 1e-6,
    );

    println!("freehand smoothing");
    let amplitude_before = (raw[1].y - raw[0].y).abs();
    let amplitude_after = (smoothed[1].y - smoothed[0].y).abs();
    check(
        "streamline removes pointer jitter",
        amplitude_after < amplitude_before,
    );

    println!("serialisation");
    let json = scene_to_json(&scene, &gpui_excalidraw::core::AppState::default()).unwrap();
    check(
        "JSON declares the excalidraw type",
        json.contains("\"type\": \"excalidraw\""),
    );
    check("JSON version is 2", json.contains("\"version\": 2"));
    check(
        "JSON contains every element",
        json.matches("\"type\": \"").count() >= 8,
    );
    check(
        "arrow binding survives serialisation",
        json.contains("\"endBinding\"") && json.contains("\"fixedPoint\""),
    );

    println!("export");
    let svg = export_svg(&scene, "#ffffff");
    check(
        "svg is well formed",
        svg.starts_with("<svg") && svg.ends_with("</svg>"),
    );
    check("svg contains paths", svg.matches("<path").count() >= 8);
    check("svg contains text", svg.contains("<text"));
    std::fs::write("verify.svg", &svg).expect("write verify.svg");
    std::fs::write("verify.excalidraw", &json).expect("write verify.excalidraw");

    println!("png export");
    let png = gpui_excalidraw::render::export::export_png(&scene, "#ffffff", 2.0)
        .expect("rasterising the scene to png");
    check(
        "png carries the png signature (not an svg renamed)",
        png.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
    );
    let png_1x = gpui_excalidraw::render::export::export_png(&scene, "#ffffff", 1.0)
        .expect("rasterising at scale 1");
    check("scale changes the raster", png.len() != png_1x.len());
    std::fs::write("verify.png", &png).expect("write verify.png");

    println!("i18n coverage");
    let mut keys = gpui_excalidraw::Editor::palette_labels();
    keys.extend(gpui_excalidraw::Editor::help_labels());
    let mut untranslated = Vec::new();
    for language in gpui_excalidraw::Language::all() {
        let i18n = gpui_excalidraw::I18n::new(language);
        for key in &keys {
            if i18n.t(key) == *key {
                untranslated.push(format!("{language:?}: {key}"));
            }
        }
    }
    check(
        &format!(
            "every palette/help label is translated in all {} languages",
            gpui_excalidraw::Language::all().len()
        ),
        untranslated.is_empty(),
    );

    println!("\nall checks passed");
    println!("wrote verify.svg, verify.png and verify.excalidraw");
}
