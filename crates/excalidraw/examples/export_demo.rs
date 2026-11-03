use gpui_excalidraw::{
    core::{
        AppState, ElementOptions, Point, Scene, new_arrow, new_ellipse, new_rectangle, new_text,
    },
    export_svg, scene_to_json,
};

fn main() {
    let mut scene = Scene::new();

    let blue = ElementOptions {
        stroke_color: "#1971c2".to_string(),
        ..Default::default()
    };
    scene.add(new_rectangle(80.0, 80.0, 200.0, 120.0, &blue));

    let red = ElementOptions {
        stroke_color: "#e03131".to_string(),
        ..Default::default()
    };
    scene.add(new_ellipse(360.0, 80.0, 140.0, 140.0, &red));

    let arrow = ElementOptions {
        stroke_color: "#e8590c".to_string(),
        ..Default::default()
    };
    scene.add(new_arrow(
        vec![Point::new(80.0, 300.0), Point::new(400.0, 300.0)],
        None,
        Some(gpui_excalidraw::core::Arrowhead::Arrow),
        &arrow,
    ));

    let text = ElementOptions::default();
    scene.add(new_text(80.0, 380.0, "Excalidraw", 36.0, &text));

    let svg = export_svg(&scene, "#ffffff");
    println!("{}", svg);

    let json = scene_to_json(&scene, &AppState::default()).unwrap();
    std::fs::write("diagram.excalidraw", json).expect("write file");
    println!("wrote diagram.excalidraw");
}
