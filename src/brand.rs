use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Image, ImageFormat, IntoElement, ObjectFit, Styled, StyledImage, img, px};
use std::sync::{Arc, OnceLock};

pub fn logo(size: f32, cx: &App) -> impl IntoElement {
    static DARK: OnceLock<Arc<Image>> = OnceLock::new();
    static LIGHT: OnceLock<Arc<Image>> = OnceLock::new();
    let image = if cx.theme().is_dark() {
        LIGHT.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/logo-light.png").to_vec(),
            ))
        })
    } else {
        DARK.get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/logo-dark.png").to_vec(),
            ))
        })
    };
    img(image.clone())
        .size(px(size))
        .object_fit(ObjectFit::Contain)
}
