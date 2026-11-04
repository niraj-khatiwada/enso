//! Floating surfaces: modal panels, hover cards and small toolbars. They share
//! one background, border and shadow and differ only in corner radius.

use gpui_kit::component::Theme;
use gpui_kit::{BoxShadow, Hsla, Pixels, Styled, black, px, white};

/// Corner radius of a modal panel (the command palette, the object editor).
pub const MODAL_RADIUS: Pixels = px(16.);
/// Corner radius of a hover card or popover.
const CARD_RADIUS: Pixels = px(12.);
/// Corner radius of a small floating toolbar.
const TOOLBAR_RADIUS: Pixels = px(8.);
/// How dark the backdrop behind a modal gets once it is fully open.
pub const BACKDROP_OPACITY: f32 = 0.25;

fn border(theme: &Theme) -> Hsla {
    if theme.mode.is_dark() {
        white().opacity(0.10)
    } else {
        theme.border
    }
}

fn shadow(theme: &Theme) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(
            px(0.),
            px(6.),
            // A dark window needs a much deeper shadow to read as depth.
            black().opacity(if theme.mode.is_dark() { 0.45 } else { 0.20 }),
        )
        .blur_radius(px(24.))
        .spread_radius(px(-4.)),
    ]
}

pub trait SurfaceExt: Styled + Sized {
    fn surface(self, theme: &Theme, radius: Pixels) -> Self {
        self.rounded(radius)
            .bg(theme.popover)
            .border_1()
            .border_color(border(theme))
            .shadow(shadow(theme))
    }

    /// A modal panel over a dimmed window.
    fn modal_surface(self, theme: &Theme) -> Self {
        self.surface(theme, MODAL_RADIUS)
    }

    /// A card that floats over the content, such as a hover preview.
    fn card_surface(self, theme: &Theme) -> Self {
        self.surface(theme, CARD_RADIUS)
    }

    /// A compact row of controls floating over the content.
    fn toolbar_surface(self, theme: &Theme) -> Self {
        self.surface(theme, TOOLBAR_RADIUS)
    }
}

impl<E: Styled> SurfaceExt for E {}
