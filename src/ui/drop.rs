//! Where a drag will land: a thin accent line with a soft glow that grows out
//! from its middle while it's the target, and shrinks away once it isn't.
//! Used between tabs and beside sidebar folders.

use gpui_kit::base::{Transition, transition};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AnyElement, App, BoxShadow, ElementId, IntoElement, Styled, Window, div, px, relative,
};
use gpui_kit::prelude::FluentBuilder;

/// The line at full length.
const LENGTH: f32 = 18.;

/// A vertical drop line, absolutely positioned by the caller (`left`/`right`).
/// `id` keys its animation; `shown` says whether the drag is over this spot.
pub fn drop_line(
    id: impl Into<ElementId>,
    shown: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme();
    let color = theme.ring;
    let motion = theme.motion_tokens();
    let policy = Transition::new(motion.duration_fast).easing(motion.easing_move.clone());
    let length = transition(id.into(), if shown { LENGTH } else { 0. }, policy, window, cx).max(0.);
    div()
        .absolute()
        .top(relative(0.5))
        .mt(px(-length / 2.))
        .w(px(2.))
        .h(px(length))
        .rounded_full()
        .bg(color)
        .opacity((length / LENGTH).clamp(0., 1.))
        .when(length > 0.5, |line| {
            line.shadow(vec![
                BoxShadow::new(px(0.), px(0.), color.opacity(0.45)).blur_radius(px(6.)),
            ])
        })
        .into_any_element()
}
