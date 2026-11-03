use std::path::Path;

use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::document::Kind;
use super::image::{ImageAlign, ImageBlock};
use super::view::{BODY_SIZE, VisualView};
use crate::tokens;
use gpui_kit::assets::IconName;

const MIN_WIDTH: f32 = tokens::MIN_IMAGE_WIDTH;

#[derive(Clone, Copy, Debug)]
pub struct ImageResize {
    pub block: u64,
    pub start_x: Pixels,
    pub start_width: Pixels,
    pub width: Pixels,
    pub centered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageTarget {
    Insert(usize),
    Replace(u64),
}

pub struct ImageDialog {
    pub target: ImageTarget,
    pub src: Entity<InputState>,
    pub alt: Entity<InputState>,
    pub width: Entity<InputState>,
    pub align: ImageAlign,
    pub title: Option<String>,
    pub error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ImageDialog {
    pub fn image(&self, cx: &App) -> Result<ImageBlock, String> {
        let url = self.src.read(cx).value().trim().to_string();
        if url.is_empty() {
            return Err("Choose a file or enter a URL.".into());
        }
        let width_text = self.width.read(cx).value().trim().to_string();
        let width = if width_text.is_empty() {
            None
        } else {
            match width_text.trim_end_matches("px").trim().parse::<u32>() {
                Ok(w) if w >= MIN_WIDTH as u32 => Some(w),
                _ => {
                    return Err(format!(
                        "Width is a number of pixels, at least {MIN_WIDTH}."
                    ));
                }
            }
        };
        Ok(ImageBlock {
            url,
            alt: self.alt.read(cx).value().trim().to_string(),
            title: self.title.clone(),
            width,
            align: self.align,
        })
    }
}

pub fn resolve_image(base: Option<&Path>, url: &str) -> ImageSource {
    match base {
        Some(base) if !url.contains(':') => ImageSource::from(base.join(url)),
        _ => ImageSource::from(SharedUri::from(url.to_string())),
    }
}

fn relative_to(base: Option<&Path>, path: &Path) -> String {
    base.and_then(|base| path.strip_prefix(base).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

impl VisualView {
    pub fn image_block_index(&self, id: u64) -> Option<usize> {
        self.doc.blocks.iter().position(|b| b.id == id)
    }

    pub fn selected_image(&self) -> Option<(usize, ImageBlock)> {
        let ix = self.image_block_index(self.selected_image?)?;
        match &self.doc.blocks[ix].kind {
            Kind::Image(image) => Some((ix, image.clone())),
            _ => None,
        }
    }

    pub fn select_image(&mut self, id: Option<u64>, cx: &mut Context<Self>) {
        if self.selected_image != id {
            self.selected_image = id;
            cx.notify();
        }
    }

    pub fn write_image(
        &mut self,
        target: ImageTarget,
        image: &ImageBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let markup = image.markup();
        let at = match target {
            ImageTarget::Replace(id) => {
                let Some(ix) = self.image_block_index(id) else {
                    return;
                };
                let block = &self.doc.blocks[ix];
                let range = block.start..block.end;
                self.edit(range.clone(), &markup, window, cx);
                range.start
            }
            ImageTarget::Insert(offset) => {
                let text = self.editor.read(cx).text().to_string();
                let offset = offset.min(text.len());
                let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
                let before = if end == 0 || text[..end].ends_with("\n\n") {
                    ""
                } else if text[..end].ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                let after = if text[end..].starts_with("\n\n") || end == text.len() {
                    ""
                } else if text[end..].starts_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                let insert = format!("{before}{markup}{after}");
                self.edit(end..end, &insert, window, cx);
                end + before.len()
            }
        };
        self.selected_image = self
            .doc
            .blocks
            .iter()
            .find(|b| b.start <= at && at < b.end.max(b.start + 1))
            .filter(|b| matches!(b.kind, Kind::Image(_)))
            .map(|b| b.id);
        cx.notify();
    }

    pub fn delete_selected_image(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((ix, _)) = self.selected_image() else {
            return false;
        };
        let text = self.editor.read(cx).text().to_string();
        let block = &self.doc.blocks[ix];
        let mut range = block.start..block.end;
        let trailing = text[range.end..]
            .chars()
            .take_while(|c| *c == '\n' || *c == '\r')
            .count();
        if range.end + trailing < text.len() {
            range.end += trailing;
        } else {
            let leading = text[..range.start]
                .chars()
                .rev()
                .take_while(|c| *c == '\n' || *c == '\r')
                .count();
            range.start -= leading;
            range.end += trailing;
        }
        self.selected_image = None;
        self.edit(range, "", window, cx);
        true
    }

    pub fn set_image_align(
        &mut self,
        align: ImageAlign,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((ix, mut image)) = self.selected_image() {
            image.align = align;
            let id = self.doc.blocks[ix].id;
            self.write_image(ImageTarget::Replace(id), &image, window, cx);
        }
    }

    pub fn open_image_dialog(
        &mut self,
        target: ImageTarget,
        existing: Option<ImageBlock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = existing.unwrap_or_default();
        let src = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("pictures/photo.png or https://…")
                .default_value(existing.url.clone())
        });
        let alt = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Describe the image")
                .default_value(existing.alt.clone())
        });
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Natural size")
                .default_value(existing.width.map(|w| w.to_string()).unwrap_or_default())
        });
        let mut subscriptions = Vec::new();
        for input in [&src, &alt, &width] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        if let Some(dialog) = &mut this.image_dialog {
                            dialog.error = None;
                            cx.notify();
                        }
                    }
                    InputEvent::PressEnter { .. } => this.confirm_image_dialog(window, cx),
                    _ => {}
                },
            ));
        }
        src.update(cx, |state, cx| state.focus(window, cx));
        self.image_dialog = Some(ImageDialog {
            target,
            src,
            alt,
            width,
            align: existing.align,
            title: existing.title,
            error: None,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    pub fn insert_image_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected_image() {
            Some((ix, image)) => {
                let id = self.doc.blocks[ix].id;
                self.open_image_dialog(ImageTarget::Replace(id), Some(image), window, cx);
            }
            None => {
                let at = self.caret;
                self.open_image_dialog(ImageTarget::Insert(at), None, window, cx);
            }
        }
    }

    pub fn close_image_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.image_dialog.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub fn confirm_image_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.image_dialog else {
            return;
        };
        match dialog.image(cx) {
            Ok(image) => {
                let target = dialog.target;
                self.image_dialog = None;
                window.focus(&self.focus, cx);
                self.write_image(target, &image, window, cx);
            }
            Err(error) => {
                dialog.error = Some(error);
                cx.notify();
            }
        }
    }

    fn choose_image_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let base = self.base.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let value = relative_to(base.as_deref(), &path);
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(dialog) = &this.image_dialog {
                    dialog
                        .src
                        .update(cx, |state, cx| state.set_value(value, window, cx));
                }
            });
        })
        .detach();
    }

    pub fn image_resize_move(&mut self, x: Pixels, cx: &mut Context<Self>) -> bool {
        let Some(resize) = &mut self.image_resize else {
            return false;
        };
        let column = self.list.viewport_bounds().size.width.max(px(MIN_WIDTH));
        let delta = (x - resize.start_x) * if resize.centered { 2. } else { 1. };
        resize.width = (resize.start_width + delta).clamp(px(MIN_WIDTH), column);
        cx.notify();
        true
    }

    pub fn image_resize_end(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(resize) = self.image_resize.take() else {
            return false;
        };
        if let Some(ix) = self.image_block_index(resize.block)
            && let Kind::Image(image) = &self.doc.blocks[ix].kind
        {
            let mut image = image.clone();
            image.width = Some(f32::from(resize.width).round() as u32);
            self.write_image(ImageTarget::Replace(resize.block), &image, window, cx);
        }
        true
    }

    pub fn render_image_block(
        &self,
        id: u64,
        image: &ImageBlock,
        pad_top: Pixels,
        pad_bottom: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_image == Some(id);
        let toolbar = selected.then(|| self.image_toolbar(image.align, cx));
        let theme = cx.theme().clone();
        let width = match self.image_resize {
            Some(resize) if resize.block == id => Some(resize.width),
            _ => image.width.map(|w| px(w as f32)),
        };
        let source = resolve_image(self.base.as_deref(), &image.url);
        let store = self.image_bounds.clone();
        let muted = theme.muted_foreground;
        let ring = theme.ring;
        let surface = theme.secondary;
        let url = image.url.clone();
        let centered = image.align == ImageAlign::Center;
        let picture = img(source)
            .max_w_full()
            .when_some(width, |img, width| img.w(width))
            .rounded(tokens::RADIUS_SM)
            .with_loading(move || {
                div()
                    .w(tokens::PLACEHOLDER_WIDTH)
                    .h(tokens::PREVIEW_HEIGHT)
                    .rounded(tokens::RADIUS_SM)
                    .bg(surface)
                    .into_any_element()
            })
            .with_fallback(move || {
                div()
                    .px_3()
                    .py_2()
                    .rounded(tokens::RADIUS_SM)
                    .bg(surface)
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Image not found: {url}"))
                    .into_any_element()
            });
        let handle = selected.then(|| {
            div()
                .id(("image-resize", id))
                .absolute()
                .right(tokens::HANDLE_INSET)
                .bottom(tokens::HANDLE_INSET)
                .size(tokens::HANDLE)
                .rounded_full()
                .bg(ring)
                .border_2()
                .border_color(theme.background)
                .cursor_ew_resize()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.focus(&this.focus, cx);
                        let start = this
                            .image_bounds
                            .borrow()
                            .get(&id)
                            .map_or(tokens::PLACEHOLDER_WIDTH, |b| b.size.width);
                        this.image_resize = Some(ImageResize {
                            block: id,
                            start_x: event.position.x,
                            start_width: start,
                            width: start,
                            centered,
                        });
                        cx.notify();
                    }),
                )
        });
        div()
            .w_full()
            .pt(pad_top + px(BODY_SIZE * 0.4))
            .pb(pad_bottom + px(BODY_SIZE * 0.6))
            .flex()
            .when(centered, |row| row.justify_center())
            .when(image.align == ImageAlign::Right, |row| row.justify_end())
            .child(
                div()
                    .id(("image", id))
                    .relative()
                    .max_w_full()
                    .cursor_pointer()
                    .rounded(tokens::RADIUS_MD)
                    .border_2()
                    .border_color(if selected {
                        ring
                    } else {
                        gpui_kit::transparent_black()
                    })
                    .child(picture)
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                store.borrow_mut().insert(id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children(toolbar)
                    .children(handle)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.focus(&this.focus, cx);
                            this.anchor = None;
                            this.select_image(Some(id), cx);
                            if event.click_count >= 2
                                && let Some((_, image)) = this.selected_image()
                            {
                                this.open_image_dialog(
                                    ImageTarget::Replace(id),
                                    Some(image),
                                    window,
                                    cx,
                                );
                            }
                        }),
                    ),
            )
            .into_any_element()
    }

    fn image_toolbar(&self, align: ImageAlign, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let button = |id: &'static str, icon: IconName, tip: &'static str, active: bool| {
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(tip)
                .accessibility_label(tip)
                .when(active, |b| b.selected(true))
        };
        let aligns = [
            (
                "image-left",
                IconName::TextAlignStart,
                "Align left",
                ImageAlign::Left,
            ),
            (
                "image-center",
                IconName::TextAlignCenter,
                "Center",
                ImageAlign::Center,
            ),
            (
                "image-right",
                IconName::TextAlignEnd,
                "Float right",
                ImageAlign::Right,
            ),
        ];
        div()
            .absolute()
            .top_2()
            .right_2()
            .flex()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .rounded_md()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .children(aligns.into_iter().map(|(id, icon, tip, value)| {
                let target = if align == value {
                    ImageAlign::Inline
                } else {
                    value
                };
                button(id, icon, tip, align == value).on_click(cx.listener(
                    move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.set_image_align(target, window, cx);
                    },
                ))
            }))
            .child(
                div()
                    .w(tokens::HAIRLINE)
                    .h(tokens::ICON)
                    .mx_0p5()
                    .bg(theme.border),
            )
            .child(
                button("image-edit", IconName::SquarePen, "Edit image…", false).on_click(
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.insert_image_dialog(window, cx);
                    }),
                ),
            )
            .child(
                button("image-delete", IconName::Trash, "Remove image", false).on_click(
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.delete_selected_image(window, cx);
                    }),
                ),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element()
    }

    pub fn render_image_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.image_dialog.as_ref()?;
        let theme = cx.theme().clone();
        let preview_url = dialog.src.read(cx).value().trim().to_string();
        let inserting = matches!(dialog.target, ImageTarget::Insert(_));
        let html = dialog.image(cx).is_ok_and(|image| image.needs_html());
        let muted = theme.muted_foreground;
        let surface = theme.secondary;
        let field = |label: &'static str, input: &Entity<InputState>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(muted).child(label))
                .child(Input::new(input).small())
        };
        let preview = if preview_url.is_empty() {
            div()
                .h(tokens::PREVIEW_HEIGHT)
                .w_full()
                .rounded_md()
                .bg(surface)
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("No image yet")
                .into_any_element()
        } else {
            let url = preview_url.clone();
            div()
                .h(tokens::PREVIEW_HEIGHT)
                .w_full()
                .rounded_md()
                .bg(surface)
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .child(
                    img(resolve_image(self.base.as_deref(), &preview_url))
                        .max_h(tokens::PREVIEW_HEIGHT - tokens::SPACE_4)
                        .max_w_full()
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(move || {
                            div()
                                .px_3()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("Can't load {url}"))
                                .into_any_element()
                        }),
                )
                .into_any_element()
        };
        let align = dialog.align;
        let aligns = h_flex_row(ImageAlign::ALL.into_iter().map(|value| {
            Button::new(SharedString::from(format!(
                "dialog-align-{}",
                value.label()
            )))
            .ghost()
            .xsmall()
            .label(value.label())
            .selected(value == align)
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(dialog) = &mut this.image_dialog {
                    dialog.align = value;
                    cx.notify();
                }
            }))
            .into_any_element()
        }));
        let note = if html {
            "Sized or aligned images are saved as an HTML <img> tag."
        } else {
            "Saved as a Markdown image."
        };
        Some(
            div()
                .id("image-dialog-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("image-dialog")
                        .w(tokens::DIALOG_WIDTH)
                        .max_w_full()
                        .p_5()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .rounded_lg()
                        .bg(theme.popover)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_lg()
                        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                            if inserting {
                                "Insert image"
                            } else {
                                "Edit image"
                            },
                        ))
                        .child(preview)
                        .child(
                            div()
                                .flex()
                                .items_end()
                                .gap_2()
                                .child(div().flex_1().child(field("File or URL", &dialog.src)))
                                .child(
                                    Button::new("image-choose")
                                        .small()
                                        .icon(IconName::FolderOpen)
                                        .label("Choose…")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.choose_image_file(window, cx)
                                        })),
                                ),
                        )
                        .child(field("Description", &dialog.alt))
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(
                                    div()
                                        .w(tokens::FIELD_NARROW)
                                        .child(field("Width (px)", &dialog.width)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(div().text_xs().text_color(muted).child("Alignment"))
                                        .child(aligns),
                                ),
                        )
                        .child(div().text_xs().text_color(muted).child(note))
                        .when_some(dialog.error.clone(), |dialog, error| {
                            dialog.child(div().text_xs().text_color(theme.danger).child(error))
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("image-cancel")
                                        .small()
                                        .label("Cancel")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_image_dialog(window, cx)
                                        })),
                                )
                                .child(
                                    Button::new("image-confirm")
                                        .small()
                                        .primary()
                                        .label(if inserting { "Insert" } else { "Save" })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.confirm_image_dialog(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn h_flex_row(children: impl Iterator<Item = AnyElement>) -> Div {
    div().flex().items_center().gap_0p5().children(children)
}
