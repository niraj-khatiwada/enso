use gpui_excalidraw::ui::design::{DARK, LIGHT, Palette, Tokens, size};

#[test]
fn primary_colours_match_upstream() {
    assert_eq!(LIGHT.primary, 0x6965db, "light --color-primary");
    assert_eq!(DARK.primary, 0xa8a5ff, "dark --color-primary");
    assert_eq!(LIGHT.canvas_bg, 0xffffff, "light --default-bg-color");
    assert_eq!(DARK.canvas_bg, 0x121212, "dark --default-bg-color");
    assert_eq!(LIGHT.island_bg, 0xffffff, "light --island-bg-color");
    assert_eq!(DARK.island_bg, 0x232329, "dark --island-bg-color");
}

#[test]
fn selected_tool_button_uses_the_primary_container_pair() {
    for tokens in [LIGHT, DARK] {
        assert_ne!(
            tokens.selected_bg(),
            tokens.selected_fg(),
            "checked tool needs contrast between its fill and its glyph"
        );
        assert_ne!(tokens.selected_bg(), tokens.island());
    }
}

#[test]
fn dark_palette_is_actually_dark_and_light_is_actually_light() {
    let luminance = |c: u32| {
        let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
        0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
    };
    assert!(luminance(LIGHT.canvas_bg) > 200.0);
    assert!(luminance(DARK.canvas_bg) < 60.0);

    assert!(luminance(LIGHT.on_surface) < 60.0);
    assert!(luminance(DARK.on_surface) > 200.0);
}

#[test]
fn palette_toggles_round_trip() {
    assert_eq!(Palette::Light.toggled(), Palette::Dark);
    assert_eq!(Palette::Dark.toggled(), Palette::Light);
    assert!(!Palette::Light.is_dark() && Palette::Dark.is_dark());
    assert_eq!(Tokens::for_palette(Palette::Dark).palette, Palette::Dark);
    assert_eq!(
        Tokens::for_palette(Palette::Light).island_bg,
        LIGHT.island_bg
    );
}

#[test]
fn sizes_match_what_excalidraw_com_actually_renders() {
    assert_eq!(size::CONTAINER_PADDING, 16.0);
    assert_eq!(size::BUTTON_LG, 36.0, "tool and chrome buttons");
    assert_eq!(size::ICON, 16.0, "every 36px button holds a 16px glyph");
    assert_eq!(size::TOOLBAR_PADDING, 4.0, "island is 4 + 36 + 4 = 44 tall");
    assert_eq!(size::TOOLBAR_GAP, 4.0, "button pitch is 40px");
    assert_eq!(size::RADIUS_LG, 8.0);

    assert_eq!(size::BUTTON, 32.0);
}

#[test]
fn the_toolbar_is_the_only_drop_shadowed_surface() {
    for tokens in [LIGHT, DARK] {
        assert_ne!(
            tokens.surface_lowest(),
            tokens.surface_low(),
            "the chip's ring has to be visible against its own fill"
        );
    }

    assert_ne!(
        LIGHT.surface_low(),
        LIGHT.island(),
        "in light a chip must read as a different surface from the toolbar"
    );
    assert_eq!(LIGHT.surface_low, 0xececf4, "light --color-surface-low");
    assert_eq!(
        LIGHT.surface_lowest, 0xffffff,
        "light --color-surface-lowest"
    );
    assert_eq!(DARK.surface_low, 0x232329, "dark --color-surface-low");
    assert_eq!(DARK.surface_lowest, 0x121212, "dark --color-surface-lowest");

    assert_eq!(LIGHT.surface_high, 0xf1f0ff, "--color-surface-high");
    assert_eq!(LIGHT.separator(), LIGHT.surface_high());

    let ring = gpui_excalidraw::ui::design::chip_ring_shadow(LIGHT);
    assert_eq!(ring.len(), 1, "the ring is a single shadow layer");
    assert_eq!(ring[0].blur_radius, gpui_kit::px(0.0), "a ring has no blur");
    assert_eq!(ring[0].spread_radius, gpui_kit::px(1.0), "exactly 1px wide");
    assert!(!ring[0].inset, "the ring sits outside the fill");
}

#[test]
fn toolbar_buttons_leave_room_for_their_glyph() {
    const { assert!(size::BUTTON_LG > size::ICON) };

    const { assert!(size::KEYBINDING_FONT <= size::ICON) };
    const { assert!(size::KEYBINDING_TOP > size::BUTTON_LG / 2.0) };
    const { assert!(size::KEYBINDING_TOP + size::KEYBINDING_FONT <= size::BUTTON_LG) };
}

#[test]
fn island_shadow_layers_match_the_upstream_stack() {
    let shadows = gpui_excalidraw::ui::design::island_shadow();
    assert_eq!(shadows.len(), 3, "--shadow-island is a three-layer stack");

    assert!(shadows[0].blur_radius < shadows[1].blur_radius);
    assert!(shadows[1].blur_radius < shadows[2].blur_radius);
    assert!(shadows[2].offset.y > shadows[0].offset.y);
}
