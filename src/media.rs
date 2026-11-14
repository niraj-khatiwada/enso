//! Pictures in a note, beyond showing them: viewing one full size, copying
//! it, and saving it elsewhere. Diagrams copy and save in the light palette,
//! which reads well wherever they end up; images as their original files.

use super::*;
use std::sync::Arc;

/// What the full-size view shows.
#[derive(Clone)]
pub enum Lightbox {
    Diagram { svg: Arc<str>, width: f32, height: f32 },
    Image(PathBuf),
    /// A picture from the web, by its address.
    Web(SharedString),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiagramFormat {
    Png,
    Svg,
}

impl Enso {
    pub(crate) fn open_lightbox(&mut self, lightbox: Lightbox, window: &mut Window, cx: &mut Context<Self>) {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.closing_lightbox = None;
        self.lightbox = Some((lightbox, focus));
        cx.notify();
    }

    pub(crate) fn close_lightbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.lightbox.take() {
            self.closing_lightbox = Some(open);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// A diagram as a picture on the clipboard (twice its size, sharp when
    /// pasted into a document or slide).
    pub(crate) fn copy_diagram(&mut self, source: &str, cx: &mut Context<Self>) {
        let png = crate::diagram::render(source, false).ok().and_then(|d| crate::diagram::png(&d, 2.));
        match png {
            Some(png) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    png,
                )));
                self.toast(ui::toast::done("copy-picture", "Copied diagram as an image", ""), cx);
            }
            None => self.message("Couldn't draw this diagram to copy it.", true, cx),
        }
    }

    pub(crate) fn save_diagram(
        &mut self,
        source: &str,
        format: DiagramFormat,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(drawn) = crate::diagram::render(source, false) else {
            self.message("Couldn't draw this diagram to save it.", true, cx);
            return;
        };
        let (bytes, name) = match format {
            DiagramFormat::Png => match crate::diagram::png(&drawn, 2.) {
                Some(png) => (png, "Diagram.png"),
                None => {
                    self.message("Couldn't draw this diagram to save it.", true, cx);
                    return;
                }
            },
            DiagramFormat::Svg => (drawn.svg.as_bytes().to_vec(), "Diagram.svg"),
        };
        self.save_bytes(bytes, name, folder, window, cx);
    }

    /// An image file on the clipboard, as the picture it is.
    pub(crate) fn copy_image(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| gpui_kit::ImageFormat::from_mime_type(&format!("image/{}", e.to_ascii_lowercase().replace("jpg", "jpeg").replace("svg", "svg+xml"))));
        match (format, std::fs::read(&path)) {
            (Some(format), Ok(bytes)) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(format, bytes)));
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.toast(ui::toast::done("copy-picture", "Copied Image", name), cx);
            }
            (None, _) => self.message("This kind of image can't be copied.", true, cx),
            (_, Err(error)) => self.message(format!("Couldn't read the image: {error}"), true, cx),
        }
    }

    /// Save a copy of an image file wherever the user picks.
    pub(crate) fn save_image_as(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or("Image".into());
        match std::fs::read(&path) {
            Ok(bytes) => {
                let folder = self.save_folder();
                self.save_bytes(bytes, &name, folder, window, cx)
            }
            Err(error) => self.message(format!("Couldn't read the image: {error}"), true, cx),
        }
    }

    /// A web image, downloaded in the background (with a spinner if it takes
    /// a moment), then copied like a local one.
    pub(crate) fn copy_web_image(&mut self, url: SharedString, cx: &mut Context<Self>) {
        let busy = self.busy(format!("download:{url}"), "Downloading the image…", cx);
        let source = url.to_string();
        let task = cx.background_executor().spawn(async move { crate::http::download(&source) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish(busy, None, cx);
                match result {
                    Ok((bytes, kind)) => match web_format(&url, kind.as_deref()) {
                        Some(format) => {
                            cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(format, bytes)));
                            this.toast(ui::toast::done("copy-picture", "Copied image", ""), cx);
                        }
                        None => this.message("This kind of image can't be copied.", true, cx),
                    },
                    Err(error) => this.message(format!("Couldn't download the image: {error}"), true, cx),
                }
            });
        })
        .detach();
    }

    /// A web image, downloaded, then saved wherever the user picks.
    pub(crate) fn save_web_image(&mut self, url: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let busy = self.busy(format!("download:{url}"), "Downloading the image…", cx);
        let source = url.to_string();
        let task = cx.background_executor().spawn(async move { crate::http::download(&source) });
        let folder = self.save_folder();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish(busy, None, cx);
                match result {
                    Ok((bytes, _)) => {
                        let name = url
                            .split(['?', '#'])
                            .next()
                            .and_then(|path| path.rsplit('/').next())
                            .filter(|name| !name.is_empty())
                            .unwrap_or("Image")
                            .to_string();
                        this.save_bytes(bytes, &name, folder, window, cx);
                    }
                    Err(error) => this.message(format!("Couldn't download the image: {error}"), true, cx),
                }
            });
        })
        .detach();
    }

    /// Where Save dialogs open: the active note's folder.
    pub(crate) fn save_folder(&self) -> Option<PathBuf> {
        self.active
            .and_then(|i| self.documents[i].path.as_ref())
            .and_then(|p| p.parent())
            .map(PathBuf::from)
    }

    fn save_bytes(
        &mut self,
        bytes: Vec<u8>,
        name: &str,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = folder
            .or_else(dirs::picture_dir)
            .or_else(dirs::document_dir)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let receiver = cx.prompt_for_new_path(&folder, Some(name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(target))) = receiver.await else {
                return;
            };
            let file = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let written = std::fs::write(&target, bytes);
            let _ = this.update(cx, |this, cx| match written {
                Ok(()) => {
                    let reveal = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in File Explorer" };
                    let note = ui::toast::done(format!("saved:{}", target.display()), "Saved", file)
                        .action(IconName::FolderOpen, reveal, move |_, cx| cx.reveal_path(&target));
                    this.toast(note, cx);
                }
                Err(error) => this.message(format!("Couldn't save “{file}”: {error}"), true, cx),
            });
        })
        .detach();
    }

    /// The full-size view: the picture as large as the window allows, over
    /// a dimmed window. A click anywhere or Escape closes it.
    pub(crate) fn lightbox_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let easing = cx.theme().motion_tokens().easing_move.clone();
        let presence = transition(
            "lightbox-presence",
            if self.lightbox.is_some() { 1.0_f32 } else { 0.0 },
            Transition::new(std::time::Duration::from_millis(200)).easing(easing),
            window,
            cx,
        )
        .clamp(0., 1.);
        if self.lightbox.is_none() && presence <= 0.001 {
            self.closing_lightbox = None;
            return None;
        }
        let (lightbox, focus) = self.lightbox.as_ref().or(self.closing_lightbox.as_ref())?.clone();
        let theme = cx.theme().clone();
        let viewport = window.viewport_size();
        let (max_w, max_h) = (viewport.width * 0.9, viewport.height * 0.86);
        let picture = match &lightbox {
            Lightbox::Diagram { svg, width, height } => {
                // Scaled up (or down) to fill the space, keeping its shape.
                let scale = (f32::from(max_w) / width).min(f32::from(max_h) / height).min(3.);
                let [r, g, b, _] = crate::diagram::background(theme.mode.is_dark());
                div()
                    .p_6()
                    .rounded(px(14.))
                    .bg(rgb(u32::from_be_bytes([0, r, g, b])))
                    .child(
                        img(crate::rich::diagram_image(svg))
                            .w(px(width * scale - 48.))
                            .h(px(height * scale - 48.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element()
            }
            Lightbox::Web(url) => div()
                // Its size isn't known before it loads: a clipped frame keeps
                // it from ever running past the window.
                .w(max_w)
                .h(max_h)
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(img(url.clone()).size_full().object_fit(ObjectFit::Contain))
                .into_any_element(),
            Lightbox::Image(path) => {
                // Fitted here from the file's own size, rather than trusting
                // the image element to shrink a large photo.
                let (w, h) = image::image_dimensions(path)
                    .map(|(w, h)| (w as f32, h as f32))
                    .unwrap_or((f32::from(max_w), f32::from(max_h)));
                let scale = (f32::from(max_w) / w).min(f32::from(max_h) / h).min(2.);
                // SVG dimensions are not supported by image_dimensions (and
                // can be percentages). Give the image a definite, clipped
                // containing block rather than letting its intrinsic size
                // influence the overlay's flex layout.
                div()
                    .debug_selector(|| "lightbox-image-frame".into())
                    .w(px(w * scale))
                    .h(px(h * scale))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .rounded(px(6.))
                    .child(img(path.clone())
                        .size_full()
                        .min_w_0()
                        .min_h_0()
                        .object_fit(ObjectFit::Contain))
                    .into_any_element()
            }
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .occlude()
                .key_context("Lightbox")
                .track_focus(&focus)
                .on_action(cx.listener(|this, _: &DismissLightbox, w, cx| this.close_lightbox(w, cx)))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this.close_lightbox(w, cx)))
                .bg(black().opacity(0.62 * presence))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .debug_selector(|| "lightbox-picture".into())
                        .flex_shrink_0()
                        .opacity(presence)
                        // A soft zoom in as it opens.
                        .mt(px(12. * (1. - presence)))
                        .child(picture),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(TITLEBAR_HEIGHT + 8.))
                        .right(px(16.))
                        .opacity(presence)
                        .child(
                            ui::icon_button("lightbox-close", IconName::Close, "Close")
                                .small()
                                .rounded_full()
                                .bg(black().opacity(0.4))
                                .text_color(white())
                                .on_click(cx.listener(|this, _, w, cx| this.close_lightbox(w, cx))),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// The image format of a download, from its content type or its address.
fn web_format(url: &str, content_type: Option<&str>) -> Option<gpui_kit::ImageFormat> {
    if let Some(format) = content_type.and_then(|t| gpui_kit::ImageFormat::from_mime_type(t.split(';').next()?.trim())) {
        return Some(format);
    }
    let extension = url.split(['?', '#']).next()?.rsplit('.').next()?.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        other => return gpui_kit::ImageFormat::from_mime_type(&format!("image/{other}")),
    };
    gpui_kit::ImageFormat::from_mime_type(mime)
}
