use gpui_excalidraw::{
    core::{element::Element, scene::AppState},
    ui::{
        Editor,
        bitmap::{
            bytes_from_data_url, data_url, decode_render_image, detect_mime, fit_size,
            mime_from_data_url, render_image_size,
        },
        export::{json_to_scene, scene_to_json},
    },
};

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 40, 40, 255]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn test_data_url_roundtrip() {
    let bytes = vec![1u8, 2, 3, 4, 5];
    let url = data_url("image/png", &bytes);
    assert!(url.starts_with("data:image/png;base64,"));
    assert_eq!(bytes_from_data_url(&url).unwrap(), bytes);
    assert_eq!(mime_from_data_url(&url).unwrap(), "image/png");
}

#[test]
fn test_detect_mime() {
    assert_eq!(
        detect_mime(&[0x89, b'P', b'N', b'G', 0, 0], None),
        "image/png"
    );
    assert_eq!(detect_mime(&[0xFF, 0xD8, 0xFF], None), "image/jpeg");
    assert_eq!(detect_mime(b"GIF89a", None), "image/gif");
    assert_eq!(
        detect_mime(b"<svg xmlns=\"\"></svg>", None),
        "image/svg+xml"
    );
    assert_eq!(
        detect_mime(b"not-an-image", Some("/tmp/a.webp")),
        "image/webp"
    );
    assert_eq!(
        detect_mime(b"not-an-image", Some("/tmp/a.jpeg")),
        "image/jpeg"
    );
    assert_eq!(detect_mime(b"not-an-image", None), "image/png");
}

#[test]
fn test_fit_size() {
    assert_eq!(fit_size(100.0, 50.0), (100.0, 50.0));
    assert_eq!(fit_size(2048.0, 1024.0), (1024.0, 512.0));
    assert_eq!(fit_size(0.0, 0.0), (200.0, 150.0));
}

#[test]
fn test_decode_render_image() {
    let render = decode_render_image(&png_bytes(4, 3), "image/png").unwrap();
    assert_eq!(render_image_size(&render), (4.0, 3.0));
    assert!(decode_render_image(b"garbage", "image/png").is_none());
}

#[test]
fn test_insert_image_bytes_registers_file_and_element() {
    let mut editor = Editor::new();
    let element_id = editor
        .insert_image_bytes(&png_bytes(8, 6), "image/png", None)
        .unwrap();

    assert_eq!(editor.document.files.len(), 1);
    let file_id = editor.document.files.keys().next().unwrap().clone();
    let file = editor.document.files.get(&file_id).unwrap();
    assert_eq!(file.mime_type, "image/png");
    assert!(file.data_url.starts_with("data:image/png;base64,"));
    assert_eq!(editor.image_dimensions(&file_id), Some((8.0, 6.0)));

    let element = editor.document.scene.get(&element_id).unwrap();
    match element {
        Element::Image(image) => assert_eq!(image.file_id.as_deref(), Some(file_id.as_str())),
        other => panic!("expected image element, got {:?}", other.kind()),
    }

    let svg = editor.export_svg_string();
    assert!(svg.contains("<image"));
    assert!(svg.contains("data:image/png;base64,"));
}

#[test]
fn test_insert_image_scales_down_large_bitmap() {
    let mut editor = Editor::new();
    let element_id = editor
        .insert_image_bytes(&png_bytes(2048, 1024), "image/png", None)
        .unwrap();
    let element = editor.document.scene.get(&element_id).unwrap();
    match element {
        Element::Image(image) => {
            assert_eq!(image.base.width, 1024.0);
            assert_eq!(image.base.height, 512.0);
        }
        other => panic!("expected image element, got {:?}", other.kind()),
    }
}

#[test]
fn test_insert_image_paths_reads_files() {
    let mut path = std::env::temp_dir();
    path.push(format!("excalidraw-image-{}.png", std::process::id()));
    std::fs::write(&path, png_bytes(10, 10)).unwrap();

    let mut editor = Editor::new();
    let inserted = editor.insert_image_paths(&[path.clone()]);
    std::fs::remove_file(&path).ok();

    assert_eq!(inserted, 1);
    assert_eq!(editor.document.files.len(), 1);
    assert_eq!(editor.document.scene.len(), 1);
}

#[test]
fn test_json_roundtrip_preserves_files() {
    let mut editor = Editor::new();
    editor
        .insert_image_bytes(&png_bytes(5, 5), "image/png", None)
        .unwrap();

    let state = AppState {
        files: editor.document.files.clone(),
        ..Default::default()
    };
    let json = scene_to_json(&editor.document.scene, &state).unwrap();
    assert!(json.contains("\"files\""));
    assert!(json.contains("\"mimeType\""));
    assert!(json.contains("\"dataURL\""));
    assert!(!json.contains("\"dataUrl\""));

    let (scene, app) = json_to_scene(&json).unwrap();
    assert_eq!(scene.len(), 1);
    assert_eq!(app.files.len(), 1);
    assert!(
        app.files
            .values()
            .all(|f| f.data_url.starts_with("data:image/png;base64,"))
    );
}

#[test]
fn test_export_png_with_embedded_image() {
    let mut editor = Editor::new();
    editor
        .insert_image_bytes(&png_bytes(16, 16), "image/png", None)
        .unwrap();
    let png = editor.export_png_bytes(1.0).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}
