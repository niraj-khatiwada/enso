use gpui_kit::{
    base::{ObservedElement, TestSupportExt},
    *,
};

use crate::{
    design::{Tokens, chip_ring_shadow, island_shadow, size},
    icons::icon,
};

pub(crate) type Button = ObservedElement<Stateful<Div>>;

pub fn island(tokens: Tokens) -> Div {
    div()
        .bg(tokens.island())
        .rounded(px(size::RADIUS_LG))
        .shadow(island_shadow())
}

pub fn toolbar_island(tokens: Tokens) -> Div {
    island(tokens)
        .flex()
        .flex_row()
        .items_center()
        .p(px(size::TOOLBAR_PADDING))
        .gap(px(size::TOOLBAR_GAP))
}

pub fn rail_island(tokens: Tokens) -> Div {
    div()
        .bg(tokens.surface_low())
        .rounded(px(size::RADIUS_LG))
        .shadow(chip_ring_shadow(tokens))
        .flex()
        .flex_row()
        .items_center()
}

pub fn toolbar_divider(tokens: Tokens, margin_right: bool) -> Div {
    let rule = div()
        .flex_shrink_0()
        .w(px(1.0))
        .h(px(24.0))
        .bg(tokens.separator());
    if margin_right {
        rule.mr(px(4.0))
    } else {
        rule.ml(px(4.0))
    }
}

pub fn kbd_chip(text: impl Into<SharedString>, tokens: Tokens) -> Div {
    div()
        .flex_shrink_0()
        .mx(px(1.0))
        .px(px(size::KBD_PAD_X))
        .py(px(size::KBD_PAD_Y))
        .rounded(px(size::KBD_RADIUS))
        .border_1()
        .border_color(tokens.shortcut_text())
        .font_family("monospace")
        .text_size(px(size::KBD_FONT))
        .text_color(tokens.shortcut_text())
        .child(text.into())
}

fn keybinding_badge(text: &str, checked: bool, tokens: Tokens) -> Div {
    let color = if checked {
        tokens.selected_fg()
    } else {
        tokens.shortcut_text()
    };
    div()
        .absolute()
        .top(px(size::KEYBINDING_TOP))
        .right(px(size::KEYBINDING_RIGHT))
        .text_size(px(size::KEYBINDING_FONT))
        .text_color(color)
        .child(SharedString::from(text.to_owned()))
}

pub fn tool_button(
    id: impl Into<ElementId>,
    icon_name: &str,
    badge: Option<&str>,
    checked: bool,
    tokens: Tokens,
) -> Button {
    let fg = if checked {
        tokens.selected_fg()
    } else {
        tokens.surface_text()
    };
    let base = div()
        .id(id)
        .test_support()
        .relative()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size::BUTTON_LG))
        .rounded(px(size::RADIUS_LG))
        .text_color(fg)
        .child(icon(icon_name, size::ICON, fg));
    let base = if checked {
        base.bg(tokens.selected_bg())
    } else {
        base.hover(move |s| s.bg(tokens.hover()))
    };
    match badge {
        Some(text) => base.child(keybinding_badge(text, checked, tokens)),
        None => base,
    }
}

pub fn chrome_button(
    id: impl Into<ElementId>,
    icon_name: &str,
    enabled: bool,
    tokens: Tokens,
) -> Button {
    let fg = if enabled {
        tokens.surface_text()
    } else {
        tokens.surface_text().alpha(0.35)
    };
    div()
        .id(id)
        .test_support()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size::BUTTON_LG))
        .rounded(px(size::RADIUS_LG))
        .text_color(fg)
        .child(icon(icon_name, size::ICON, fg))
        .hover(move |s| s.bg(tokens.hover()))
}

pub fn zoom_readout(tokens: Tokens) -> Div {
    div()
        .flex_shrink_0()
        .h(px(size::BUTTON_LG))
        .min_w(px(60.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(16.0))
        .text_color(tokens.surface_text())
}

pub fn menu_row(
    id: impl Into<ElementId>,
    icon_name: Option<&str>,
    label: &str,
    tokens: Tokens,
) -> Button {
    menu_row_with(id, icon_name, label, None, false, tokens)
}

pub fn menu_row_with(
    id: impl Into<ElementId>,
    icon_name: Option<&str>,
    label: &str,
    shortcut: Option<&str>,
    checked: bool,
    tokens: Tokens,
) -> Button {
    menu_row_colored(
        id,
        icon_name,
        label,
        shortcut,
        checked,
        tokens,
        tokens.surface_text(),
    )
}

pub fn menu_row_colored(
    id: impl Into<ElementId>,
    icon_name: Option<&str>,
    label: &str,
    shortcut: Option<&str>,
    checked: bool,
    tokens: Tokens,
    fg: Rgba,
) -> Button {
    let row = div()
        .id(id)
        .test_support()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .w_full()
        .min_h(px(size::MENU_ROW))
        .px(px(8.0))
        .rounded(px(size::RADIUS_MD))
        .text_size(px(14.0))
        .text_color(fg)
        .hover(move |s| s.bg(tokens.hover()));

    let row = match icon_name {
        Some(name) => row.child(icon(name, size::ICON, fg)),
        None if checked => row.child(icon("checkIcon", size::ICON, fg)),
        None => row.child(div().flex_shrink_0().w(px(size::ICON))),
    };
    let row = row.child(div().flex_1().child(SharedString::from(label.to_owned())));
    match shortcut {
        Some(keys) => row.child(
            div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .text_color(tokens.shortcut_text())
                .child(SharedString::from(keys.to_owned())),
        ),
        None => row,
    }
}

pub fn dropdown(tokens: Tokens) -> Div {
    div()
        .absolute()
        .flex()
        .flex_col()
        .p(px(8.0))
        .gap(px(2.0))
        .min_w(px(264.0))
        .occlude()
        .bg(tokens.island())
        .rounded(px(size::RADIUS_LG))
        .shadow(island_shadow())
}
