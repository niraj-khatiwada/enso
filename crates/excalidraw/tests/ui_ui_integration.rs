use gpui_excalidraw::ui::Editor;
use gpui_kit::{
    AppContext, Bounds, Entity, Point, TestAppContext, WindowBounds, WindowHandle, WindowOptions,
    px, size, test::TestWindowExt,
};

fn open_editor(cx: &mut TestAppContext) -> (WindowHandle<gpui_kit::base::Root>, Entity<Editor>) {
    open_editor_sized(cx, 800.0, 600.0)
}

fn open_editor_sized(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (WindowHandle<gpui_kit::base::Root>, Entity<Editor>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(width), px(height)),
        };
        let (window, content) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let mut editor = Editor::new();
                editor.add_demo_scene();
                cx.new(|_| editor)
            },
        )
        .expect("open test window");
        (
            window.downcast::<gpui_kit::base::Root>().expect("root"),
            content,
        )
    })
}

#[gpui_kit::test]
fn editor_renders_toolbar_and_canvas(cx: &mut TestAppContext) {
    let (window, _content) = open_editor(cx);
    cx.update_window(window.into(), |_, _window, cx| {
        _window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn chrome_islands_are_laid_out_and_clickable(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        for id in [
            "menu",
            "tool-lock",
            "tool-handIcon",
            "tool-SelectionIcon",
            "tool-RectangleIcon",
            "tool-DiamondIcon",
            "tool-EllipseIcon",
            "tool-ArrowIcon",
            "tool-LineIcon",
            "tool-FreedrawIcon",
            "tool-TextIcon",
            "tool-stickyNoteToolIcon",
            "tool-EraserIcon",
            "more-tools",
            "library",
            "theme",
            "zoom-out",
            "zoom-in",
            "undo",
            "redo",
        ] {
            let snapshot = window.find(id);
            assert!(snapshot.visible(), "{id} is not visible");
            let bounds = snapshot.bounds();
            assert!(
                bounds.size.width >= px(20.0) && bounds.size.height >= px(20.0),
                "{id} is too small to hit: {bounds:?}"
            );
        }

        window.click("tool-lock", cx);
        content.update(cx, |editor, _| {
            assert!(editor.document.tool_locked, "the lock click never arrived");
        });
        window.click("tool-lock", cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.document.tool_locked,
                "the lock click did not toggle"
            );
        });

        let before = content.read(cx).document.scene.len();
        window.click("tool-RectangleIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Rectangle);
            assert_eq!(
                editor.document.scene.len(),
                before,
                "the canvas reacted to a toolbar click"
            );
        });

        window.click("more-tools", cx);
        window.render_frame(cx);
        assert!(
            window.find("more-bucketFillIcon").visible(),
            "the bucket lives in the extra tools menu, like upstream"
        );
        window.click("more-laserPointerToolIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Laser);
            assert!(!editor.is_more_tools_open(), "the popover should close");
        });

        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-lassoToolIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.tool,
                Tool::Lasso,
                "the lasso row arms the lasso tool"
            );
            assert!(!editor.is_more_tools_open(), "the popover should close");
        });

        window.click("tool-EllipseIcon", cx);
        let before = content.read(cx).document.scene.len();
        window.drag(
            Point::new(px(400.0), px(300.0)),
            Point::new(px(520.0), px(400.0)),
            cx,
        );
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                before + 1,
                "dragging on the canvas should add an ellipse"
            );
            assert_eq!(
                editor.document.tool,
                Tool::Selection,
                "an unlocked tool hands back the selection tool"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_web_embed_tool_draws_an_embeddable_and_asks_for_its_link(cx: &mut TestAppContext) {
    use gpui_excalidraw::{core::element::ElementType, ui::Tool};

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-EmbedIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.tool,
                Tool::Embeddable,
                "the row arms the embeddable tool instead of inserting an element"
            );
            assert!(!editor.is_more_tools_open(), "the popover should close");
        });

        let before = content.read(cx).document.scene.len();
        window.drag(
            Point::new(px(300.0), px(250.0)),
            Point::new(px(520.0), px(430.0)),
            cx,
        );
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                before + 1,
                "dragging on the canvas should add one embeddable"
            );
            let element = editor
                .document
                .scene
                .elements
                .last()
                .expect("the drawn embeddable");
            assert_eq!(element.kind(), ElementType::Embeddable);
            assert!(
                element.base().width > 200.0 && element.base().height > 150.0,
                "the drag, not a fixed default, sizes the frame"
            );
            assert!(
                editor.is_link_dialog_open(),
                "upstream asks for a link right after the frame is drawn"
            );
            assert_eq!(
                editor.document.tool,
                Tool::Selection,
                "an unlocked tool hands back the selection tool"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_link_popup_shows_the_summary_and_edits_on_pencil(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-EmbedIcon", cx);
        window.drag(
            Point::new(px(300.0), px(250.0)),
            Point::new(px(520.0), px(430.0)),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(editor.is_link_editing(), "a fresh frame opens the input");
        });

        window.click("link-confirm", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                editor.is_link_dialog_open(),
                "submitting keeps the popup open in summary mode"
            );
            assert!(!editor.is_link_editing());
        });
        assert!(
            window.try_find("link-input").is_none(),
            "the summary shows the label instead of the input"
        );
        assert!(
            window.find("link-edit").visible(),
            "the summary popup offers the pencil button"
        );
        assert!(
            window.find("link-label").visible(),
            "the summary names the missing link"
        );

        window.click("link-edit", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(editor.is_link_editing(), "the pencil reopens the input");
        });
        assert!(
            window.find("link-input").visible(),
            "the pencil brings the input back"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_laser_trail_outlives_the_pointer_and_decays_from_its_tail(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-laserPointerToolIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Laser);
        });

        window.drag(
            Point::new(px(300.0), px(250.0)),
            Point::new(px(520.0), px(430.0)),
            cx,
        );
        content.update(cx, |editor, _| {
            let trail = editor.laser_trail();
            assert!(
                trail.len() >= 2,
                "releasing must keep the trail instead of clearing it"
            );
            assert!(
                trail.last().expect("newest sample").1 >= trail.first().expect("oldest sample").1,
                "samples decay from the oldest end"
            );
            assert!(
                trail.iter().all(|(_, remaining)| *remaining > 0.0),
                "fresh samples are still visible"
            );
            assert!(!editor.is_interacting(), "the stroke has ended");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_bucket_tool_fills_an_enclosed_region(cx: &mut TestAppContext) {
    use gpui_excalidraw::{
        core::{FillStyle, element::Element},
        ui::Tool,
    };

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        window.click("tool-RectangleIcon", cx);
        window.drag(
            Point::new(px(300.0), px(250.0)),
            Point::new(px(520.0), px(430.0)),
            cx,
        );

        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-bucketFillIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::BucketFill);
            assert_eq!(
                editor.document.style.fill_style,
                FillStyle::Solid,
                "upstream defaults the bucket fill style to solid"
            );
        });

        // Upstream's bucket panel is exactly colour, fill style and opacity.
        assert!(window.find("legend-background").visible());
        assert!(window.find("legend-fill").visible());
        assert!(window.find("opacity-slider").visible());
        content.update(cx, |editor, _| {
            assert_ne!(
                editor.i18n.t("labels.background"),
                "labels.background",
                "the background legend is missing its translation"
            );
        });
        window.click("fill-hachure", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.style.fill_style, FillStyle::Hachure);
        });

        let click = Point::new(px(400.0), px(330.0));
        let before = content.read(cx).document.scene.len();
        window.drag(click, click, cx);

        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                before + 1,
                "a click inside the rectangle paints one polygon"
            );
            let fill = editor
                .document
                .scene
                .elements
                .last()
                .expect("the generated fill");
            match fill {
                Element::Line(line) => {
                    assert!(line.polygon, "fills are closed polygons");
                    assert_eq!(line.linear.base.stroke_color, "transparent");
                    assert_eq!(
                        line.linear.base.background_color, "#b2f2bb",
                        "a transparent background falls back to the green swatch"
                    );
                    assert_eq!(
                        line.linear.base.fill_style,
                        FillStyle::Hachure,
                        "the panel's fill style applies to the painted fill"
                    );
                    assert!(
                        line.linear.points.len() >= 4,
                        "the traced outline needs a real polygon"
                    );
                }
                other => panic!("expected a polygon line, got {:?}", other.kind()),
            }
            assert_eq!(
                editor.document.tool,
                Tool::BucketFill,
                "upstream keeps the bucket armed for repeated fills"
            );
            assert!(
                editor.document.selected.is_empty(),
                "painting does not select the generated fill"
            );
            let fill_id = editor
                .document
                .scene
                .elements
                .last()
                .unwrap()
                .id()
                .to_string();
            assert_eq!(
                editor
                    .document
                    .element_at(gpui_excalidraw::Point::new(400.0, 330.0), 2.0),
                Some(fill_id),
                "the painted interior is selectable afterwards"
            );
        });

        window.drag(click, click, cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                before + 1,
                "re-clicking existing paint restyles it instead of stacking a duplicate"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn hamburger_menu_rows_are_clickable(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("menu", cx);
        content.update(cx, |editor, _| assert!(editor.is_menu_open()));

        window.render_frame(cx);
        window.click("fm-prefs", cx);
        content.update(cx, |editor, _| assert!(editor.is_menu_open()));

        window.render_frame(cx);
        window.click("prefs-grid", cx);
        content.update(cx, |editor, _| {
            assert!(editor.document.show_grid, "the grid row did nothing");
            assert!(editor.is_menu_open(), "the submenu should stay open");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn hamburger_menu_is_capped_and_scrolls(cx: &mut TestAppContext) {
    let height = 380.0;
    let (window, _content) = open_editor_sized(cx, 800.0, height);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("menu", cx);
        window.render_frame(cx);

        let menu = window.find("file-menu").bounds();

        let cap = height - 32.0 - 36.0;
        assert!(
            menu.size.height <= px(cap),
            "the menu should stop short of the window edge, got {menu:?}"
        );

        assert!(
            !window.find("fm-lang").visible(),
            "with the cap in place the language select starts below the fold"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn language_select_is_a_listbox_not_a_cycle(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Language;

    let (window, content) = open_editor_sized(cx, 800.0, 900.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("menu", cx);
        window.render_frame(cx);

        let before = content.read(cx).i18n.language();
        window.click("fm-lang", cx);
        content.update(cx, |editor, _| {
            assert!(editor.is_language_open(), "the select never expanded");
            assert_eq!(
                editor.i18n.language(),
                before,
                "opening the list changed the language"
            );
        });

        window.render_frame(cx);
        for language in Language::all() {
            let id = gpui_kit::SharedString::from(format!("lang-{}", language.code()));
            assert!(window.try_find(id.clone()).is_some(), "{id} is missing");
        }
        let panel = window.find("lang-list").bounds();
        assert!(
            panel.size.height <= px(900.0 - 32.0 - 36.0),
            "the list should stop short of the window edge, got {panel:?}"
        );
        assert!(
            window.find("lang-en").visible(),
            "the list should open on its first row"
        );

        window.click("lang-de-DE", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.i18n.language(), Language::De);
            assert!(!editor.is_language_open(), "the list should collapse");
            assert!(editor.is_menu_open(), "the menu should stay open");
        });

        window.render_frame(cx);
        window.click("fm-lang", cx);
        window.render_frame(cx);
        window.click("lang-en", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.i18n.language(), Language::En);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_background_row_ends_with_the_custom_colour_trigger(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 800.0, 900.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("menu", cx);
        window.render_frame(cx);

        assert!(
            window.try_find("viewbg-transparent").is_none(),
            "the canvas row must not offer a transparent preset"
        );
        assert!(
            window.try_find("viewbg-custom").is_some(),
            "the canvas row should end with the custom-colour trigger"
        );

        window.click("viewbg-f8f9fa", cx);
        assert_eq!(
            content.read(cx).document.view_background_color,
            "#f8f9fa",
            "picking a tint should set the canvas background"
        );

        window.render_frame(cx);
        window.click("viewbg-custom", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("color-hex-field").is_some(),
            "the trigger should open the hex field"
        );
        assert!(
            window.try_find("color-black").is_none(),
            "the canvas-background popup must not show the palette grid"
        );
        assert!(
            window.try_find("shade-0").is_none(),
            "the canvas-background popup must not show the shade ramp"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn shape_switch_retypes_a_lone_selected_element(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::ElementType;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let (id, center) = {
            let editor = content.read(cx);
            let base = editor.document.scene.elements[2].base();
            (
                base.id.clone(),
                Point::new(
                    px((base.x + base.width / 2.0) as f32),
                    px((base.y + base.height / 2.0) as f32),
                ),
            )
        };
        window.drag(center, center, cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.selected_count(), 1, "the canvas click missed");
        });

        window.render_frame(cx);
        for control in [
            "shape-switch",
            "shape-rectangle",
            "shape-diamond",
            "shape-ellipse",
        ] {
            assert!(window.find(control).visible(), "{control} did not paint");
        }
        assert!(
            window.try_find("shape-line").is_none(),
            "a diamond must not be offered a line button"
        );

        let version = content
            .read(cx)
            .document
            .scene
            .get(&id)
            .unwrap()
            .base()
            .version;
        window.click("shape-diamond", cx);
        content.update(cx, |editor, _| {
            let element = editor.document.scene.get(&id).unwrap();
            assert_eq!(element.kind(), ElementType::Diamond);
            assert_eq!(element.base().version, version, "a no-op still bumped");
        });

        window.render_frame(cx);
        window.click("shape-rectangle", cx);
        content.update(cx, |editor, _| {
            let element = editor.document.scene.get(&id).unwrap();
            assert_eq!(
                element.kind(),
                ElementType::Rectangle,
                "the click missed the button"
            );
            assert!(
                element.base().version > version,
                "the conversion was not recorded"
            );
        });
        window.press("ctrl-z", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.get(&id).unwrap().kind(),
                ElementType::Diamond,
                "undo did not restore the shape"
            );
        });

        let ids: Vec<String> = content
            .read(cx)
            .document
            .scene
            .elements
            .iter()
            .map(|e| e.id().to_string())
            .collect();
        content.update(cx, |editor, _| {
            editor.document.select_many(ids[..2].to_vec())
        });
        window.render_frame(cx);
        assert!(
            window.try_find("shape-switch").is_none(),
            "multi-select kept the switcher"
        );

        let text_id = content
            .read(cx)
            .document
            .scene
            .elements
            .iter()
            .find(|e| e.kind() == ElementType::Text)
            .map(|e| e.id().to_string())
            .expect("the demo scene holds a text element");
        content.update(cx, |editor, _| editor.document.select(&text_id));
        window.render_frame(cx);
        assert!(
            window.try_find("shape-switch").is_none(),
            "text got a shape switcher"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_click_on_a_locked_element_offers_an_unlock_bubble(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::{Element, ElementBase, ElementType};

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let mut base = ElementBase::new("unlock-target".to_string(), ElementType::Rectangle);
        base.x = 300.0;
        base.y = 250.0;
        base.width = 150.0;
        base.height = 100.0;
        content.update(cx, |editor, _| {
            editor.document.scene.add(Element::Rectangle { base });

            let other = editor.document.scene.elements[1].id().to_string();
            editor
                .document
                .select_many(vec!["unlock-target".to_string(), other]);
        });

        window.render_frame(cx);
        window.click("arrange-lock", cx);
        content.update(cx, |editor, _| {
            assert!(
                editor
                    .document
                    .scene
                    .get("unlock-target")
                    .unwrap()
                    .base()
                    .locked,
                "the panel's padlock never locked the selection"
            );
        });

        window.render_frame(cx);
        let centre = Point::new(px(375.0), px(300.0));
        window.drag(centre, centre, cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.selected_count(),
                0,
                "a locked element should not become the selection directly"
            );
        });

        window.render_frame(cx);
        let popup = window.find("unlock-popup").bounds();
        let bottom_edge = popup.origin.y + popup.size.height;
        assert!(
            bottom_edge <= px(250.0) && bottom_edge >= px(230.0),
            "the padlock should sit just above the element's top edge, got {popup:?}"
        );

        window.click("unlock-popup", cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor
                    .document
                    .scene
                    .get("unlock-target")
                    .unwrap()
                    .base()
                    .locked,
                "the bubble did not unlock the element"
            );
            assert_eq!(editor.selected_count(), 1);
            assert!(editor.document.selected.contains("unlock-target"));
        });
        window.render_frame(cx);
        assert!(
            window.try_find("unlock-popup").is_none(),
            "the bubble should clear once it has done its job"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_buttons_track_a_generated_iframe(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::{
        Element, ElementBase, ElementType, FrameElement, IframeElement,
    };

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let mut base = ElementBase::new("gen-iframe".to_string(), ElementType::Iframe);
        base.x = 300.0;
        base.y = 250.0;
        base.width = 200.0;
        base.height = 150.0;
        base.link = Some("https://example.com/embed".to_string());
        base.custom_data = Some(serde_json::json!({ "generationData": { "status": "done" } }));
        content.update(cx, |editor, _| {
            editor
                .document
                .scene
                .add(Element::Iframe(IframeElement { base }));
            editor.document.select("gen-iframe");
        });

        window.render_frame(cx);
        assert!(
            window.find("canvas-copyIcon").visible(),
            "a finished generated iframe should offer a copy button"
        );
        assert!(window.find("canvas-fullscreenIcon").visible());
        let island = window.find("canvas-buttons").bounds();

        assert!(
            (island.origin.x.to_f64() - 510.0).abs() < 1.0,
            "the island should hang off the right edge, got {island:?}"
        );
        assert!(
            (island.origin.y.to_f64() - 250.0).abs() < 1.0,
            "the island should start at the element's top edge, got {island:?}"
        );

        window.click("canvas-copyIcon", cx);

        content.update(cx, |editor, _| {
            editor
                .document
                .scene
                .get_mut("gen-iframe")
                .unwrap()
                .base_mut()
                .custom_data =
                Some(serde_json::json!({ "generationData": { "status": "pending" } }));
        });
        window.render_frame(cx);
        assert!(
            window.try_find("canvas-buttons").is_none(),
            "an unfinished generation must not get the island"
        );

        content.update(cx, |editor, _| {
            editor.document.scene.remove("gen-iframe");
            let mut base = ElementBase::new("magic".to_string(), ElementType::MagicFrame);
            base.x = 300.0;
            base.y = 250.0;
            base.width = 200.0;
            base.height = 150.0;
            editor
                .document
                .scene
                .add(Element::MagicFrame(FrameElement { base, name: None }));
            editor.document.select("magic");
        });
        window.render_frame(cx);
        assert!(window.find("canvas-MagicIcon").visible());
        assert!(
            window.try_find("canvas-copyIcon").is_none(),
            "a magic frame is not a copy-source element"
        );
    })
    .unwrap();

    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("https://example.com/embed"));
}

#[gpui_kit::test]
fn a_selected_frame_hides_the_shape_style_controls(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::{Element, ElementBase, ElementType, FrameElement};

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            let mut base = ElementBase::new("frame-1".to_string(), ElementType::Frame);
            base.x = 100.0;
            base.y = 100.0;
            base.width = 300.0;
            base.height = 200.0;
            editor
                .document
                .scene
                .add(Element::Frame(FrameElement { base, name: None }));
            editor.document.select("frame-1");
        });
        window.render_frame(cx);

        assert!(
            window.try_find("opacity-slider").is_some(),
            "frames keep the opacity control"
        );
        assert!(
            window.try_find("layers-front").is_some(),
            "frames keep the layer controls"
        );
        assert!(
            window.try_find("single-del").is_some(),
            "frames keep the action controls"
        );
        assert!(
            window.try_find("width-thin").is_none(),
            "frames must not offer a stroke width"
        );
        assert!(
            window.try_find("fill-hachure").is_none(),
            "frames must not offer a fill style"
        );
        assert!(
            window.try_find("edge-sharp").is_none(),
            "frames must not offer a roundness"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_shortcuts_survive_goroot_level_dispatch(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        window.press("r", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.tool,
                Tool::Rectangle,
                "the r shortcut never reached the editor"
            );
        });

        window.press("e", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Eraser, "the e shortcut is dead");
        });

        window.press("v", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Selection, "v is dead");
        });

        window.click("menu", cx);
        content.update(cx, |editor, _| assert!(editor.is_menu_open()));
        window.press("escape", cx);
        content.update(cx, |editor, _| {
            assert!(!editor.is_menu_open(), "escape did not reach the editor");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_z_undoes_through_the_real_dispatch_path(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = content.read(cx).document.scene.len();

        window.click("tool-RectangleIcon", cx);
        window.drag(
            Point::new(px(400.0), px(300.0)),
            Point::new(px(500.0), px(380.0)),
            cx,
        );
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.scene.len(), before + 1);
            assert_eq!(editor.document.tool, Tool::Selection);
        });

        window.press("ctrl-z", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                before,
                "ctrl-z never reached the editor"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_slash_filters_and_runs_the_command_palette(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        window.press("ctrl-/", cx);
        content.update(cx, |editor, _| assert!(editor.is_palette_open()));

        window.render_frame(cx);
        assert!(
            window.find("palette-hints").visible(),
            "the palette should show its shortcut hints"
        );
        for category in ["App", "Export", "Editor", "Tools"] {
            let id = gpui_kit::SharedString::from(format!("palette-section-{category}"));
            assert!(
                window.try_find(id.clone()).is_some(),
                "{id} heading is missing"
            );
        }
        assert!(
            window.try_find("palette-section-Elements").is_none(),
            "element commands need a selection, so the group must stay hidden"
        );

        let panel = window.find("palette-panel").bounds();
        let hints = window.find("palette-hints").bounds();
        let commands = window.find("palette-commands").bounds();
        assert!(
            panel.size.width >= px(700.0),
            "the dialog should use upstream's 720px width, got {panel:?}"
        );
        assert!(
            hints.origin.y < commands.origin.y,
            "the hint strip sits above the command list"
        );

        window.input("rect", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.palette_query(), "rect");
        });
        assert!(
            window.try_find("palette-row-1").is_none(),
            "the filter should collapse the list to a single match"
        );
        assert!(
            window.try_find("palette-section-Tools").is_some(),
            "the rectangle tool lives in the Tools group"
        );

        window.press("enter", cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.is_palette_open(),
                "running a command should dismiss it"
            );
            assert_eq!(editor.document.tool, Tool::Rectangle);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_rows_are_clickable_over_the_backdrop(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("tool-RectangleIcon", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.document.tool, Tool::Rectangle)
        });

        window.click("menu", cx);
        window.render_frame(cx);
        window.click("fm-palette", cx);
        content.update(cx, |editor, _| assert!(editor.is_palette_open()));

        window.render_frame(cx);
        window.input("rect", cx);
        window.render_frame(cx);
        window.click("palette-row-0", cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.is_palette_open(),
                "the click should land on the row, not the backdrop"
            );
            assert_eq!(editor.document.tool, Tool::Rectangle);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_escape_dismisses_without_running(cx: &mut TestAppContext) {
    use gpui_excalidraw::ui::Tool;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-/", cx);
        content.update(cx, |editor, _| assert!(editor.is_palette_open()));

        window.press("escape", cx);
        content.update(cx, |editor, _| {
            assert!(!editor.is_palette_open());
            assert_eq!(
                editor.document.tool,
                Tool::Selection,
                "escape ran a command"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn palette_lists_the_upstream_command_set(cx: &mut TestAppContext) {
    let _ = cx;

    let labels = Editor::palette_labels();

    for expected in [
        "labels.toggleTheme",
        "toolBar.library",
        "search.title",
        // rows the released palette adds on top of the open-source list
        "labels.liveCollaboration",
        "labels.share",
        "overwriteConfirm.action.excalidrawPlus.button",
        // export rows, in upstream declaration order
        "buttons.exportImage",
        "exportDialog.disk_title",
        "labels.copyAsPng",
        "labels.copyAsSvg",
        "toolBar.lock",
    ] {
        assert!(labels.contains(&expected), "palette is missing {expected}");
    }

    assert!(
        !labels.contains(&"menu.open"),
        "upstream has no open row in the palette"
    );
}

#[gpui_kit::test]
fn palette_hides_website_only_commands(cx: &mut TestAppContext) {
    let (window, _content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-/", cx);
        window.render_frame(cx);
        window.input("collab", cx);
        window.render_frame(cx);

        assert!(
            window.try_find("palette-row-0").is_none(),
            "live collaboration has no handler here, so it isn't listed"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn save_raises_the_export_dialog(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("save-dialog-backdrop").is_none(),
            "the dialog must not be up before it is asked for"
        );

        window.click("menu", cx);
        window.render_frame(cx);
        window.click("fm-save", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(editor.save_dialog_open(), "the menu row raised nothing");
            let untitled = editor.i18n.t("labels.untitled");

            assert!(
                editor.project_name_value().starts_with(&untitled),
                "the name field should show App.getName()'s fallback, got {:?}",
                editor.project_name_value()
            );
        });

        assert!(window.find("save-dialog-backdrop").visible());
        let field = window.find("save-filename").bounds();
        assert!(
            field.size.width > px(120.0) && field.size.width < px(240.0),
            "the name field is the wrong width: {field:?}"
        );

        let field_centre = field.origin.x + field.size.width / 2.0;
        assert!(
            (field_centre - px(500.0)).abs() < px(12.0),
            "the dialog is not centred: {field:?}"
        );
        let button = window.find("save-card-button").bounds();
        assert!(
            button.size.width > px(60.0) && button.size.width < px(240.0),
            "the card's button should hug its label: {button:?}"
        );
        assert!(
            window.find("save-card-button").visible(),
            "the card's button is off-screen"
        );

        let seeded = content.update(cx, |editor, _| editor.project_name_value());
        window.input("x", cx);
        content.update(cx, |editor, _| {
            assert!(
                editor.project_name_value().ends_with('x'),
                "typing did not reach the name field: {:?}",
                editor.project_name_value()
            );
        });
        assert_eq!(
            window.find("save-filename").value(),
            Some(format!("{seeded}x").as_str()),
            "the field did not repaint after a keystroke"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.save_dialog_open(),
                "escape did not dismiss the modal"
            );
        });

        window.press("ctrl-s", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(editor.save_dialog_open(), "Ctrl+S raised nothing");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn export_dialog_moves_the_caret_and_selects_all(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-s", cx);
        window.render_frame(cx);

        let seeded = content.update(cx, |editor, _| editor.project_name_value());

        // Home moves the caret, so the next character is inserted in front.
        window.press("home", cx);
        window.input("A", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("save-filename").value(),
            Some(format!("A{seeded}").as_str()),
            "typing did not insert at the caret"
        );

        // Ctrl+A selects the whole field; the next character replaces it.
        window.press("ctrl-a", cx);
        window.input("Z", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("save-filename").value(),
            Some("Z"),
            "typing did not replace the selection"
        );
        content.update(cx, |editor, _| assert_eq!(editor.project_name_value(), "Z"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn export_image_raises_the_dialog(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("image-dialog-backdrop").is_none(),
            "the export dialog must not be up before it is asked for"
        );

        window.click("menu", cx);
        window.render_frame(cx);
        window.click("fm-png", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(editor.image_dialog_open(), "the menu row raised nothing");
        });

        assert!(window.find("image-dialog-backdrop").visible());
        for id in [
            "export-filename",
            "export-btn-png",
            "export-btn-svg",
            "export-btn-copy",
            "export-row-background",
            "export-row-dark-mode",
            "export-row-embed-scene",
            "export-scale-1",
        ] {
            assert!(window.find(id).visible(), "{id} is missing from the dialog");
        }

        let preview = window.find("export-preview").bounds();
        let settings = window.find("export-settings").bounds();
        assert!(
            preview.size.width > settings.size.width,
            "the preview should keep the wider 55% column: {preview:?} vs {settings:?}"
        );
        assert!(
            preview.size.height > px(300.0),
            "the preview is shorter than upstream's 360px box: {preview:?}"
        );
        let field = window.find("export-filename").bounds();
        assert!(
            field.origin.y > preview.origin.y + preview.size.height,
            "the name field belongs under the preview, not beside it: {field:?}"
        );
        let png = window.find("export-btn-png").bounds();
        let svg = window.find("export-btn-svg").bounds();
        let copy = window.find("export-btn-copy").bounds();
        assert!(
            (png.origin.y - svg.origin.y).abs() < px(1.0)
                && (svg.origin.y - copy.origin.y).abs() < px(1.0),
            "the three action buttons should sit on one row: {png:?} {svg:?} {copy:?}"
        );
        assert!(
            copy.origin.x + copy.size.width <= settings.origin.x + settings.size.width + px(1.0),
            "the action buttons overflow the settings column: {copy:?} {settings:?}"
        );

        // Toggling a setting flips the state and keeps the dialog up.
        window.click("export-row-dark-mode", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                editor.export_dark_mode(),
                "the dark mode switch did not flip"
            );
        });

        // Choosing a scale updates the export scale.
        window.click("export-scale-2", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.export_scale(), 2.0);
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.image_dialog_open(),
                "escape did not dismiss the export modal"
            );
        });

        window.press("ctrl-shift-e", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(editor.image_dialog_open(), "Ctrl+Shift+E raised nothing");
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_f_finds_and_cycles_text_matches(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-f", cx);
        content.update(cx, |editor, _| assert!(editor.is_find_open()));

        window.input("e", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.find_query(), "e");
            assert_eq!(editor.find_match_count(), 2);
            assert_eq!(editor.find_index(), 0);
        });

        window.press("enter", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.find_index(), 1, "enter did not step")
        });

        window.press("enter", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.find_index(), 0, "the stepper should wrap");
        });

        window.press("escape", cx);
        content.update(cx, |editor, _| assert!(!editor.is_find_open()));
    })
    .unwrap();
}

#[gpui_kit::test]
fn help_dialog_opens_on_question_mark(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("shift-/", cx);
        content.update(cx, |editor, _| assert!(editor.is_help_open()));

        window.render_frame(cx);
        assert!(
            window.find("help-close").visible(),
            "the help dialog did not paint"
        );

        // Upstream splits the shortcuts into a left column (tools, view) and a
        // right column (editor) spanning both rows.
        let left = window.find("help-col-left").bounds();
        let right = window.find("help-col-right").bounds();
        assert!(
            left.origin.x < right.origin.x,
            "tools/view and editor must sit in separate columns"
        );
        assert!(
            left.size.width > px(100.0) && right.size.width > px(100.0),
            "both island columns should be laid out with real width"
        );

        window.press("escape", cx);
        content.update(cx, |editor, _| assert!(!editor.is_help_open()));
    })
    .unwrap();
}

#[gpui_kit::test]
fn png_export_from_the_editor_is_a_real_png(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let bytes = content
            .update(cx, |editor, _| editor.export_png_bytes(2.0))
            .expect("export should succeed");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG signature");

        let path = std::env::temp_dir().join("excalidraw-ui-test-export.png");
        let path_string = path.to_string_lossy().into_owned();
        content
            .update(cx, |editor, _| editor.export_png_to(&path_string))
            .expect("writing the PNG should succeed");
        let written = std::fs::read(&path).expect("the exported file should exist");
        assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");
        let _ = std::fs::remove_file(&path);
    })
    .unwrap();
}

#[gpui_kit::test]
fn eye_dropper_picks_the_colour_under_the_pointer(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);

        let before = content
            .read(cx)
            .document
            .scene
            .get(&target)
            .unwrap()
            .base()
            .stroke_color
            .clone();
        let expected = content.read(cx).export_background().to_ascii_lowercase();

        window.click("color-trigger-stroke", cx);
        window.render_frame(cx);
        window.click("eye-dropper-stroke", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                editor.is_eye_dropper_active(),
                "the trigger never armed the picker"
            );
        });
        assert!(
            window.find("eye-dropper").visible(),
            "the backdrop did not paint"
        );

        window.click_at("eye-dropper", Point::new(px(400.0), px(275.0)), cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.is_eye_dropper_active(),
                "clicking should end the pick"
            );
            let stroke = &editor
                .document
                .scene
                .get(&target)
                .unwrap()
                .base()
                .stroke_color;
            assert_ne!(stroke, &before, "the picked colour never landed");
            assert_eq!(
                stroke.to_ascii_lowercase(),
                expected,
                "the pick should have sampled the canvas background"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn escape_abandons_an_eye_dropper_pick(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);
        let before = content
            .read(cx)
            .document
            .scene
            .get(&target)
            .unwrap()
            .base()
            .stroke_color
            .clone();

        window.click("color-trigger-stroke", cx);
        window.render_frame(cx);
        window.click("eye-dropper-stroke", cx);
        content.update(cx, |editor, _| assert!(editor.is_eye_dropper_active()));

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.is_eye_dropper_active(),
                "escape should cancel the pick"
            );
            assert_eq!(
                editor
                    .document
                    .scene
                    .get(&target)
                    .unwrap()
                    .base()
                    .stroke_color,
                before,
                "a cancelled pick must not write a colour"
            );
        });
        assert!(
            window.try_find("eye-dropper").is_none(),
            "the backdrop should be torn down"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn arrow_shortcut_shows_a_hint_beside_the_pointer(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        window.drag(
            Point::new(px(400.0), px(300.0)),
            Point::new(px(400.0), px(300.0)),
            cx,
        );

        window.press("a", cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.cursor_hint_icon(),
                Some("sharpArrowIcon"),
                "the letter shortcut should hint the arrow tool"
            );
        });
        assert!(
            window.find("cursor-hint").visible(),
            "the hint did not paint"
        );

        window.drag(
            Point::new(px(420.0), px(320.0)),
            Point::new(px(420.0), px(320.0)),
            cx,
        );
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.cursor_hint_icon(),
                None,
                "a press should hide the hint"
            );
        });

        window.press("5", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.cursor_hint_icon(), Some("sharpArrowIcon"));
        });
    })
    .unwrap();

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(900));
    cx.run_until_parked();
    let icon = cx
        .update_window(window.into(), |_, _, cx| {
            content.read(cx).cursor_hint_icon()
        })
        .unwrap();
    assert_eq!(icon, None, "the hint should auto-hide");
}

#[gpui_kit::test]
fn external_file_drag_shows_the_overlay_and_a_scene_drop_replaces(cx: &mut TestAppContext) {
    use std::path::PathBuf;

    use gpui_kit::{ExternalPaths, FileDropEvent, PlatformInput};

    let (window, content) = open_editor(cx);

    let path = std::env::temp_dir().join("gpui-excalidraw-ui-drop-test.excalidraw");
    let path_string = path.to_string_lossy().into_owned();
    cx.update(|cx| {
        content
            .update(cx, |editor, _| editor.save_to(&path_string))
            .expect("exporting the demo scene should succeed");
    });
    let saved_len = cx.update(|cx| content.read(cx).document.scene.len());

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let paths = ExternalPaths([PathBuf::from(&path_string)].into_iter().collect());
        window.dispatch_event(
            PlatformInput::FileDrop(FileDropEvent::Entered {
                position: Point::new(px(400.0), px(300.0)),
                paths,
            }),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.file_drag_kind(),
                Some("scene"),
                "a .excalidraw file is a scene"
            );
        });
        assert!(window.find("file-drop-overlay").visible());

        window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Exited), cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| assert_eq!(editor.file_drag_kind(), None));
        assert!(window.try_find("file-drop-overlay").is_none());

        content.update(cx, |editor, _| editor.document.scene.elements.clear());
        window.render_frame(cx);

        let paths = ExternalPaths([PathBuf::from(&path_string)].into_iter().collect());
        window.dispatch_event(
            PlatformInput::FileDrop(FileDropEvent::Entered {
                position: Point::new(px(400.0), px(300.0)),
                paths,
            }),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            PlatformInput::FileDrop(FileDropEvent::Submit {
                position: Point::new(px(400.0), px(300.0)),
            }),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.scene.len(),
                saved_len,
                "the drop did not load the file"
            );
            let message = editor
                .toast_message()
                .expect("the replace should report itself");
            assert!(
                message.contains("Content replaced"),
                "unexpected toast text: {message}"
            );
        });
        assert!(window.find("toast").visible(), "the toast did not paint");

        window.click("toast-close", cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.toast_message(), None, "the close button did nothing");
        });
    })
    .unwrap();

    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
fn colour_picker_popup_writes_the_clicked_swatch(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);

        assert!(
            window.try_find("color-red").is_none(),
            "the palette should not be painted before the trigger is clicked"
        );

        window.click("color-trigger-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-red").visible(),
            "the trigger never opened the palette"
        );
        assert!(
            window.find("color-hex-field").visible(),
            "the hex row is missing"
        );
        assert!(
            window.find("shade-0").visible(),
            "a ramped colour should offer its ramp"
        );

        window.click("color-red", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor
                    .document
                    .scene
                    .get(&target)
                    .unwrap()
                    .base()
                    .stroke_color,
                "#e03131",
                "clicking a swatch should write that colour"
            );
        });

        window.click("shade-0", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor
                    .document
                    .scene
                    .get(&target)
                    .unwrap()
                    .base()
                    .stroke_color,
                "#fff5f5",
                "stepping the ramp should move the whole grid to that shade"
            );
        });

        window.click("color-trigger-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("color-red").is_none(),
            "clicking the trigger again should close the popup"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn font_picker_popup_searches_and_selects(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::Element;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let text_id = content
            .read(cx)
            .document
            .scene
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Text(_) => Some(element.id().to_string()),
                _ => None,
            })
            .expect("the demo scene has a text element");
        content.update(cx, |editor, _| editor.document.select(&text_id));
        window.render_frame(cx);

        assert!(
            window.try_find("font-search").is_none(),
            "the list starts closed"
        );
        window.click("font-trigger", cx);
        window.render_frame(cx);
        assert!(
            window.find("font-search").visible(),
            "the trigger did not open the list"
        );
        assert!(
            window.find("font-item-Comic Shanns").visible(),
            "every family should be listed"
        );

        window.input("comic", cx);
        window.render_frame(cx);
        assert!(window.find("font-item-Comic Shanns").visible());
        assert!(
            window.try_find("font-item-Nunito").is_none(),
            "a needle should hide the families it does not match"
        );

        window.click("font-item-Comic Shanns", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            let family = match editor.document.scene.get(&text_id).unwrap() {
                Element::Text(text) => text.font_family,
                _ => panic!("the text element changed type"),
            };
            assert_eq!(
                family,
                gpui_excalidraw::core::types::FontFamily::ComicShanns,
                "clicking a row should set the family"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn element_link_dialog_writes_and_clears_the_link(cx: &mut TestAppContext) {
    // The action row sits below the fold of a 600px window once the panel
    // grows its per-row legends, so this test uses a taller one.
    let (window, content) = open_editor_sized(cx, 800.0, 1100.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);

        assert!(
            window.try_find("link-input").is_none(),
            "the dialog starts closed"
        );
        window.click("single-link", cx);
        window.render_frame(cx);
        assert!(
            window.find("link-input").visible(),
            "the control did not open the dialog"
        );

        assert!(window.try_find("link-remove").is_none());

        window.input("example.com", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.link_input(),
                "example.com",
                "typing should reach the field"
            );
        });

        window.click("link-confirm", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(editor.selected_link().as_deref(), Some("example.com"));
        });
        assert!(
            window.try_find("link-input").is_none(),
            "confirming should dismiss the dialog"
        );

        window.click("single-link", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.link_input(),
                "example.com",
                "the field should be seeded"
            );
        });
        window.click("link-remove", cx);
        window.render_frame(cx);
        window.click("link-confirm", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.selected_link(),
                None,
                "confirming an empty field clears the link"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_v_pastes_clipboard_text_as_text_elements(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::Element;

    let (window, content) = open_editor(cx);

    cx.update(|cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
            "first\n\nsecond".to_string(),
        ));
    });

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press("ctrl-v", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            let bodies: Vec<String> = editor
                .document
                .selected_elements()
                .iter()
                .filter_map(|element| match element {
                    Element::Text(text) => Some(text.text.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                bodies,
                vec!["first".to_string(), "second".to_string()],
                "the paste should make one text element per non-blank line"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_v_while_editing_pastes_into_the_text_element(cx: &mut TestAppContext) {
    use gpui_excalidraw::{core::element::Element, ui::Tool};

    let (window, content) = open_editor(cx);

    cx.update(|cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("pasteme".to_string()));
    });

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let id = content.update(cx, |editor, _| {
            editor.set_tool(Tool::Text);
            editor.on_canvas_mouse_down(400.0, 300.0, gpui_kit::Modifiers::default(), 1);
            editor.editing_text_id().map(str::to_string)
        });
        let id = id.expect("the text tool should open a new element for editing");

        window.press("ctrl-v", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            let element = editor
                .document
                .scene
                .get(&id)
                .expect("the text element vanished");
            match element {
                Element::Text(text) => assert_eq!(text.text, "pasteme"),
                _ => panic!("the paste changed the element type"),
            }
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_opacity_row_is_a_0_to_100_slider(cx: &mut TestAppContext) {
    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);

        assert!(
            window.try_find("opacity-100").is_none(),
            "opacity must not be a row of preset buttons anymore"
        );

        let slider = window.find("opacity-slider").bounds();
        let y = px(18.0);
        let track_end = slider.size.width - px(9.0);

        window.click("opacity-slider", cx);
        window.click_at("opacity-slider", Point::new(px(8.0), y), cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.selected_elements()[0].base().opacity,
                0.0,
                "the left end of the track should be 0%"
            );
        });

        window.click_at("opacity-slider", Point::new(track_end, y), cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.selected_elements()[0].base().opacity,
                100.0,
                "the right end of the track should be 100%"
            );
        });

        // Dragging, not just clicking, has to move the value and stay on the
        // track once the pointer travels past it. One drag also collapses into
        // a single undo step, so a single undo returns the pre-drag value.
        window.drag(
            slider.origin + Point::new(slider.size.width / 2.0, y),
            slider.origin + Point::new(px(8.0), y),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert_eq!(
                editor.document.selected_elements()[0].base().opacity,
                0.0,
                "dragging across the track should read the final pointer position"
            );
        });
        window.press("ctrl-z", cx);
        content.update(cx, |editor, _| {
            // Undoing drops the selection, so the value is read back from the
            // element itself.
            let restored = editor
                .document
                .scene
                .elements
                .iter()
                .find(|element| element.id() == target.as_str())
                .expect("undo removed the dragged element");
            assert_eq!(
                restored.base().opacity,
                100.0,
                "the whole drag should have been a single undo step"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_panel_labels_its_groups_and_orders_the_actions(cx: &mut TestAppContext) {
    use gpui_excalidraw::core::element::Element;

    let (window, content) = open_editor(cx);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);

        let target = content.read(cx).document.scene.elements[0].id().to_string();
        content.update(cx, |editor, _| editor.document.select(&target));
        window.render_frame(cx);

        for id in [
            "legend-fill",
            "legend-stroke-width",
            "legend-stroke-style",
            "legend-sloppiness",
            "legend-edges",
            "legend-opacity",
            "legend-layers",
            "legend-actions",
        ] {
            assert!(window.try_find(id).is_some(), "{id} is missing");
        }

        let duplicate = window.find("single-dup").bounds();
        let delete = window.find("single-del").bounds();
        let link = window.find("single-link").bounds();
        assert!(
            duplicate.origin.x < delete.origin.x && delete.origin.x < link.origin.x,
            "the action row should read duplicate, delete, link: {duplicate:?} {delete:?} {link:?}"
        );
        assert!(
            window.try_find("single-lock").is_none(),
            "upstream does not put lock in this row"
        );

        let linear = content
            .read(cx)
            .document
            .scene
            .elements
            .iter()
            .find_map(|element| {
                matches!(element, Element::Arrow(_) | Element::Line(_))
                    .then(|| element.id().to_string())
            });
        let linear = linear.expect("the demo scene has an arrow");
        content.update(cx, |editor, _| editor.document.select(&linear));
        window.render_frame(cx);
        assert!(
            window.try_find("legend-arrowheads").is_some(),
            "linear elements label their arrowheads"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn clearing_the_canvas_asks_before_it_wipes(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = content.read(cx).document.scene.len();
        assert!(before > 0, "the demo scene should have elements");

        window.click("menu", cx);
        window.render_frame(cx);
        window.click("fm-reset", cx);
        window.render_frame(cx);

        assert!(
            window.try_find("clear-dialog-confirm").is_some(),
            "the reset row should ask before clearing"
        );
        assert_eq!(
            content.read(cx).document.scene.len(),
            before,
            "opening the dialog must not clear anything"
        );

        window.click("clear-dialog-cancel", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("clear-dialog-confirm").is_none(),
            "cancelling should close the dialog"
        );
        assert_eq!(
            content.read(cx).document.scene.len(),
            before,
            "cancelling must keep the scene"
        );

        window.click("menu", cx);
        window.render_frame(cx);
        window.click("fm-reset", cx);
        window.render_frame(cx);
        window.click("clear-dialog-confirm", cx);
        window.render_frame(cx);

        assert!(
            window.try_find("clear-dialog-confirm").is_none(),
            "confirming should close the dialog"
        );
        assert_eq!(
            content.read(cx).document.scene.len(),
            0,
            "confirming should clear the canvas"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn mermaid_dialog_opens_previews_and_inserts(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-mermaidIcon", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(
                editor.is_mermaid_dialog_open(),
                "the Mermaid row should raise the dialog"
            );
            assert!(
                !editor.mermaid_preview_elements().is_empty(),
                "the seeded example should already render a preview"
            );
        });
        assert!(window.find("mermaid-input").visible());
        assert!(window.find("mermaid-preview").visible());

        let before = content.read(cx).document.scene.len();
        window.click("mermaid-insert", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(
                !editor.is_mermaid_dialog_open(),
                "inserting should close the dialog"
            );
            assert!(
                editor.document.scene.len() > before,
                "inserting should add the generated elements"
            );
            assert!(
                !editor.document.selected.is_empty(),
                "the inserted diagram should be selected"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn mermaid_ctrl_enter_inserts_like_the_shortcut_hint(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-mermaidIcon", cx);
        window.render_frame(cx);

        let before = content.read(cx).document.scene.len();
        window.press("ctrl-enter", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(
                !editor.is_mermaid_dialog_open(),
                "Ctrl+Enter should close the dialog"
            );
            assert!(
                editor.document.scene.len() > before,
                "Ctrl+Enter should insert the diagram"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn mermaid_editor_edits_the_source_and_closes_on_escape(cx: &mut TestAppContext) {
    let (window, content) = open_editor_sized(cx, 1000.0, 800.0);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("more-tools", cx);
        window.render_frame(cx);
        window.click("more-mermaidIcon", cx);
        window.render_frame(cx);

        window.press("ctrl-a", cx);
        window.input("graph LR", cx);
        window.press("enter", cx);
        window.input("A[One] --> B[Two]", cx);
        window.render_frame(cx);

        content.update(cx, |editor, _| {
            assert!(
                editor.mermaid_source().starts_with("graph LR"),
                "typing should replace the selected source"
            );
            assert!(
                !editor.mermaid_preview_elements().is_empty(),
                "the edited source should be re-parsed"
            );
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |editor, _| {
            assert!(
                !editor.is_mermaid_dialog_open(),
                "escape should dismiss the Mermaid dialog"
            );
        });
    })
    .unwrap();
}
