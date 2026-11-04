//! Toasts: one-line confirmations and errors that rise at the bottom of the
//! window and leave on their own. Each is a pill that fits its text (up to a
//! limit), with its buttons centered on it. `Enso` keeps the list and draws
//! it (see `Enso::toast`); this module builds and paints them.

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, ElementId, InteractiveElement, IntoElement,
    ParentElement, SharedString, Styled, Transformation, Window, div,
    px, size,
};
use std::{rc::Rc, time::Duration};

use super::SurfaceExt as _;

/// The widest a toast gets; longer details end in "…".
const MAX_WIDTH: f32 = 440.;
const RADIUS: f32 = 12.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Done,
    Working,
    Error,
    /// News, such as a newer enso: an "i" in the theme's info color.
    Info,
}

/// A button on a toast: an icon, with its label as the tooltip.
#[derive(Clone)]
pub struct ToastAction {
    pub icon: IconName,
    pub label: SharedString,
    pub run: Rc<dyn Fn(&mut Window, &mut App)>,
}

#[derive(Clone)]
pub struct Toast {
    /// A toast with the same id replaces this one in place.
    pub id: SharedString,
    pub mark: Mark,
    pub title: SharedString,
    pub detail: SharedString,
    pub action: Option<ToastAction>,
    /// Leaves on its own after a few seconds.
    pub autohide: bool,
}

impl Toast {
    pub fn action(
        mut self,
        icon: IconName,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(ToastAction { icon, label: label.into(), run: Rc::new(run) });
        self
    }
}

/// Something finished: a check mark that pops in, a title and a detail.
pub fn done(id: impl Into<SharedString>, title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Toast {
    Toast {
        id: id.into(),
        mark: Mark::Done,
        title: title.into(),
        detail: detail.into(),
        action: None,
        autohide: true,
    }
}

/// Something under way: a spinner and a line of text. It stays until a toast
/// with the same id replaces it.
pub fn working(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Toast {
    Toast {
        id: id.into(),
        mark: Mark::Working,
        title: text.into(),
        detail: SharedString::default(),
        action: None,
        autohide: false,
    }
}

/// News that waits to be read: it stays until dismissed.
pub fn info(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Toast {
    Toast {
        id: id.into(),
        mark: Mark::Info,
        title: title.into(),
        detail: SharedString::default(),
        action: None,
        autohide: false,
    }
}

/// Something went wrong. The same message again replaces the toast rather
/// than stacking another.
pub fn error(text: impl Into<SharedString>) -> Toast {
    let text = text.into();
    Toast {
        id: text.clone(),
        mark: Mark::Error,
        title: text,
        detail: SharedString::default(),
        action: None,
        autohide: true,
    }
}

/// Paint one toast. `presence` runs 0 → 1 as it arrives and back to 0 as it
/// leaves: it rises and fades in, and sinks and fades out.
pub fn render(
    toast: &Toast,
    presence: f32,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let mark = match toast.mark {
        Mark::Done => Icon::new(IconName::CircleCheck)
            .size_4()
            .text_color(theme.success)
            .with_animation(
                ElementId::from((ElementId::from(toast.id.clone()), "pop")),
                Animation::new(Duration::from_millis(420)).with_easing(overshoot),
                |icon, t| {
                    let scale = 0.4 + 0.6 * t;
                    icon.transform(Transformation::scale(size(scale, scale)))
                },
            )
            .into_any_element(),
        Mark::Working => Spinner::new().xsmall().into_any_element(),
        Mark::Error => Icon::new(IconName::CircleX)
            .size_4()
            .text_color(theme.danger)
            .into_any_element(),
        Mark::Info => Icon::new(IconName::Info)
            .size_4()
            .text_color(theme.info)
            .into_any_element(),
    };
    let id = ElementId::from(toast.id.clone());
    h_flex()
        .id(id.clone())
        .occlude()
        .max_w(px(MAX_WIDTH))
        .min_w_0()
        .h(px(36.))
        .pl_3()
        .pr_1()
        .gap_2()
        .items_center()
        .text_sm()
        .surface(theme, px(RADIUS))
        .opacity(presence)
        .mb(px(-8. * (1. - presence)))
        .child(div().flex_shrink_0().flex().items_center().child(mark))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .child(toast.title.clone()),
        )
        .when(!toast.detail.is_empty(), |row| {
            row.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .child(toast.detail.clone()),
            )
        })
        .child(
            h_flex()
                .flex_shrink_0()
                .ml_1()
                .gap_0p5()
                .items_center()
                .when_some(toast.action.clone(), |row, action| {
                    row.child(
                        super::icon_button(
                            ElementId::from((id.clone(), "action")),
                            action.icon,
                            action.label.clone(),
                        )
                        .xsmall()
                        .on_click(move |_, window, cx| (action.run)(window, cx)),
                    )
                })
                .child(
                    super::icon_button(ElementId::from((id.clone(), "close")), IconName::Close, "Dismiss")
                        .xsmall()
                        .on_click(move |_, window, cx| on_close(window, cx)),
                ),
        )
        .into_any_element()
}

/// Ease out past the end and settle back: the check mark's little pop.
fn overshoot(t: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}
