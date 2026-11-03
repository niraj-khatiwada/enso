//! Buttons. Each returns the kit's `Button`, so callers can still chain a
//! size, an icon or a click handler.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{ElementId, SharedString};

/// The one action a dialog or form exists for (Apply, Insert): solid, dark
/// in light mode and light in dark mode.
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).primary().small().label(label)
}

/// A supporting action beside the content it changes (+ Row, Replace image…,
/// the welcome screen's Open file).
pub fn secondary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).small().label(label)
}

/// Dismisses a dialog without changing anything.
pub fn cancel_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).ghost().small().label("Cancel")
}

/// An icon-only button. `label` is its tooltip and what screen readers say.
/// Pick `.xsmall()` inside rows and lists, `.small()` in toolbars.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .icon(icon)
        .tooltip(label.clone())
        .accessibility_label(label)
}
