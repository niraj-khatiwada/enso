use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::*;

use crate::tokens;

pub fn floating_toolbar(id: &'static str, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_0p5()
        .p_0p5()
        .rounded_md()
        .bg(theme.popover)
        .border_1()
        .border_color(theme.border)
        .shadow_md()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

pub fn tool(id: impl Into<ElementId>, icon: IconName, tip: &'static str, active: bool) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(tip)
        .accessibility_label(tip)
        .selected(active)
}

pub fn divider(cx: &App) -> Div {
    div()
        .w(tokens::HAIRLINE)
        .h(tokens::ICON)
        .mx_0p5()
        .bg(cx.theme().border)
}
