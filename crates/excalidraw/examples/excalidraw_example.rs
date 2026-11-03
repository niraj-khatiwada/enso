use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_excalidraw::ui::icons::IconAssets)
        .run(|cx| {
            gpui_kit::init(cx);

            gpui_excalidraw::ui::fonts::register_hand_drawn_fonts(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| {
                    let mut editor = gpui_excalidraw::Editor::new();
                    editor.set_language(gpui_excalidraw::Language::ZhCn);
                    match std::env::var("EXCALIDRAW_UI_STATE").as_deref() {
                        Ok("scene") => editor.add_demo_scene(),
                        Ok("menu") => editor.set_menu_open(true),
                        Ok("prefs") => editor.debug_open_prefs(),
                        Ok("more") => editor.set_more_tools_open(true),
                        Ok("palette") => editor.open_palette(),
                        Ok("find") => {
                            editor.add_demo_scene();
                            editor.open_find();
                            editor.set_find_query("e");
                        }
                        Ok("help") => editor.open_help(),
                        _ => {}
                    }
                    editor
                })
            })
            .expect("failed to open window");
        });
}
