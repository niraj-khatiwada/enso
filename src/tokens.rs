use std::time::Duration;

use gpui_kit::{Pixels, px};

pub const SPACE_1: Pixels = px(2.);
pub const SPACE_2: Pixels = px(4.);
pub const SPACE_3: Pixels = px(6.);
pub const SPACE_4: Pixels = px(8.);
pub const SPACE_5: Pixels = px(12.);
pub const SPACE_6: Pixels = px(16.);
pub const SPACE_7: Pixels = px(24.);
pub const SPACE_8: Pixels = px(32.);

pub const RADIUS_SM: Pixels = px(4.);
pub const RADIUS_MD: Pixels = px(6.);
pub const RADIUS_LG: Pixels = px(8.);

pub const HAIRLINE: Pixels = px(1.);
pub const NUDGE: Pixels = px(0.5);

pub const CONTROL_SM: Pixels = px(20.);
pub const CONTROL: Pixels = px(28.);
pub const CONTROL_LG: Pixels = px(32.);
pub const BAR: Pixels = px(44.);
pub const STATUS_BAR: Pixels = px(20.);
pub const ICON_SM: Pixels = px(14.);
pub const ICON: Pixels = px(16.);
pub const SWATCH: Pixels = px(24.);
pub const HANDLE: Pixels = px(11.);
pub const CAPTION_TEXT: Pixels = px(10.);

pub const SIDEBAR_WIDTH: Pixels = px(248.);
pub const SIDEBAR_MIN: Pixels = px(160.);
pub const SIDEBAR_MAX: Pixels = px(480.);
pub const PANE_MIN: Pixels = px(180.);
pub const MENU_WIDTH: Pixels = px(240.);
pub const NAV_WIDTH: Pixels = px(180.);
pub const DIALOG_WIDTH: Pixels = px(420.);
pub const FIELD_NARROW: Pixels = px(120.);
pub const PREVIEW_HEIGHT: Pixels = px(140.);
pub const PLACEHOLDER_WIDTH: Pixels = px(240.);
pub const SETTING_PREVIEW: Pixels = px(66.);
pub const SETTING_CARD: Pixels = px(132.);

pub const PAGE_WIDTH: Pixels = px(680.);
pub const PAGE_TOP: f32 = 28.;
pub const PAGE_BOTTOM: f32 = 96.;
pub const BODY_TEXT: f32 = 17.;
pub const LINE_RATIO: f32 = 1.53;
pub const MIN_IMAGE_WIDTH: f32 = 48.;

pub const WINDOW: (f32, f32) = (1180., 780.);
pub const WINDOW_MIN: (f32, f32) = (800., 500.);
pub const SETTINGS_WINDOW: (f32, f32) = (760., 480.);
pub const SETTINGS_WINDOW_MIN: (f32, f32) = (680., 420.);

pub const HOVER: f32 = 0.5;
pub const ACTIVE: f32 = 0.8;
pub const SUBTLE: f32 = 0.25;
pub const MISSING: f32 = 0.55;
pub const DISABLED: f32 = 0.4;
pub const VEIL: f32 = 0.6;
pub const FOCUS_DIM: f32 = 0.62;
pub const GUIDE_REST: f32 = 0.18;
pub const GUIDE_HOVER: f32 = 0.55;
pub const GUIDE_ACTIVE: f32 = 0.7;
pub const HIDDEN: f32 = 0.;
pub const SHOWN: f32 = 1.;

pub const FOCUS_FADE: Duration = Duration::from_millis(160);
pub const CHROME_FADE: Duration = Duration::from_millis(240);

pub const SCROLL_THUMB: Pixels = px(3.);
pub const PREVIEW_SIDEBAR: Pixels = px(18.);
pub const FADED: f32 = 0.5;
pub const GUIDE_HIT: Pixels = px(13.);
pub const HANDLE_INSET: Pixels = px(5.);
pub const FLOAT_OFFSET: Pixels = px(34.);
pub const AUTOSCROLL_EDGE: Pixels = px(24.);
