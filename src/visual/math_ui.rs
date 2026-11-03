use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, EditorState, InputEvent},
};
use gpui_kit::*;

use super::document::Kind;
use super::math::{self, MathKey};
use super::view::{BODY_SIZE, VisualView};
use crate::tokens;

const WRAP_COLUMNS: usize = 52;

#[derive(Clone, Debug, PartialEq)]
pub enum MathTarget {
    Insert(usize),
    Inline(Range<usize>),
    Block(u64),
}

pub struct MathDialog {
    pub target: MathTarget,
    pub editor: Entity<EditorState>,
    pub display: bool,
    _subscription: Subscription,
}

impl VisualView {
    fn math_preview(
        &self,
        tex: &str,
        display: bool,
        size: Pixels,
        window: &Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let key = MathKey::new(tex, display, size, theme.foreground, window.scale_factor());
        match math::request(key, cx) {
            Some(Ok(math)) => img(ImageSource::Render(math.image))
                .w(math.width)
                .h(math.height)
                .max_w_full()
                .into_any_element(),
            Some(Err(error)) => div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_1)
                .text_sm()
                .child(
                    div()
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground)
                        .child(tex.to_string()),
                )
                .child(div().text_xs().text_color(theme.danger).child(error))
                .into_any_element(),
            None => div()
                .font_family(theme.mono_font_family.clone())
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(tex.to_string())
                .into_any_element(),
        }
    }

    pub fn render_math_block(
        &self,
        id: u64,
        tex: &str,
        pad_top: Pixels,
        pad_bottom: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size = px(self.text_size_px());
        let preview = self.math_preview(tex, true, size, window, cx);
        let hover = cx.theme().secondary.opacity(tokens::FADED);
        div()
            .w_full()
            .pt(pad_top + px(BODY_SIZE * 0.4))
            .pb(pad_bottom + px(BODY_SIZE * 0.6))
            .child(
                div()
                    .id(("math-block", id))
                    .w_full()
                    .py(tokens::SPACE_2)
                    .flex()
                    .justify_center()
                    .rounded(tokens::RADIUS_MD)
                    .overflow_hidden()
                    .cursor_pointer()
                    .hover(move |block| block.bg(hover))
                    .child(preview)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_math_dialog(MathTarget::Block(id), window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub fn inline_math_at(&self, at: Point<Pixels>, cx: &App) -> Option<Range<usize>> {
        let geometry = self.geometry.borrow();
        let (key, atom) = geometry
            .iter()
            .find_map(|(key, g)| g.atom_at(at).map(|atom| (*key, atom)))?;
        let (bix, lix) = self.doc.leaf_by_key(key)?;
        let start = self.doc.src_of(bix, lix, atom.start);
        let end = self.doc.src_of(bix, lix, atom.end);
        let text = self.editor.read(cx).text();
        if end <= start || end > text.len() {
            return None;
        }
        let source = text.slice(start..end).to_string();
        math::looks_like_math(&source).then_some(start..end)
    }

    fn math_source(&self, target: &MathTarget, cx: &App) -> (String, bool) {
        match target {
            MathTarget::Insert(_) => (String::new(), false),
            MathTarget::Inline(range) => {
                let text = self.editor.read(cx).text().slice(range.clone()).to_string();
                (text.trim_matches('$').to_string(), false)
            }
            MathTarget::Block(id) => {
                let tex = self
                    .doc
                    .blocks
                    .iter()
                    .find(|b| b.id == *id)
                    .and_then(|b| match &b.kind {
                        Kind::Math { tex } => Some(tex.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                (tex, true)
            }
        }
    }

    pub fn open_math_dialog(
        &mut self,
        target: MathTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tex, display) = self.math_source(&target, cx);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .folding(false)
                .line_number(false)
                .default_value(tex.trim().to_string())
        });
        editor.update(cx, |state, cx| {
            state.set_soft_wrap(true, window, cx);
            state.focus(window, cx);
        });
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.confirm_math_dialog(window, cx),
                _ => {}
            },
        );
        self.math_dialog = Some(MathDialog {
            target,
            editor,
            display,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub fn insert_math_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.selection() {
            range if !range.is_empty() => {
                let text = self.editor.read(cx).text().slice(range.clone()).to_string();
                if math::looks_like_math(&text) {
                    MathTarget::Inline(range)
                } else {
                    MathTarget::Insert(self.caret)
                }
            }
            _ => MathTarget::Insert(self.caret),
        };
        self.open_math_dialog(target, window, cx);
    }

    pub fn close_math_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.math_dialog.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub fn confirm_math_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.math_dialog.take() else {
            return;
        };
        window.focus(&self.focus, cx);
        let tex = dialog.editor.read(cx).value().trim().to_string();
        let newline = if self.editor.read(cx).text().to_string().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let markup = if dialog.display {
            format!("$${newline}{tex}{newline}$$")
        } else {
            format!("${tex}$")
        };
        match dialog.target {
            MathTarget::Block(id) => {
                let Some(block) = self.doc.blocks.iter().find(|b| b.id == id) else {
                    return;
                };
                let range = block.start..block.end;
                let text = if tex.is_empty() {
                    String::new()
                } else {
                    markup
                };
                self.edit(range, &text, window, cx);
            }
            MathTarget::Inline(range) => {
                let text = if tex.is_empty() {
                    String::new()
                } else {
                    markup
                };
                self.edit(range, &text, window, cx);
            }
            MathTarget::Insert(at) if !tex.is_empty() => {
                if dialog.display {
                    let source = self.editor.read(cx).text().to_string();
                    let at = at.min(source.len());
                    let end = source[at..].find('\n').map_or(source.len(), |i| at + i);
                    let before =
                        if end == 0 || source[..end].ends_with(&format!("{newline}{newline}")) {
                            String::new()
                        } else {
                            format!("{newline}{newline}")
                        };
                    self.edit(end..end, &format!("{before}{markup}{newline}"), window, cx);
                } else {
                    self.edit(at..at, &markup, window, cx);
                }
            }
            MathTarget::Insert(_) => {}
        }
        cx.notify();
    }

    pub fn render_math_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.math_dialog.as_ref()?;
        let theme = cx.theme().clone();
        let tex = dialog.editor.read(cx).value().to_string();
        let display = dialog.display;
        let preview = if tex.trim().is_empty() {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Type LaTeX, for example \\frac{a}{b} or \\sum_{i=1}^n x_i")
                .into_any_element()
        } else {
            self.math_preview(&tex, display, px(self.text_size_px()), window, cx)
        };
        let lines = tex
            .lines()
            .map(|line| line.chars().count().div_ceil(WRAP_COLUMNS).max(1))
            .sum::<usize>()
            .clamp(3, 10);
        let editing = !matches!(dialog.target, MathTarget::Insert(_));
        let mode = |id: &'static str, label: &'static str, value: bool| {
            Button::new(id)
                .ghost()
                .xsmall()
                .label(label)
                .selected(display == value)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = &mut this.math_dialog {
                        dialog.display = value;
                        cx.notify();
                    }
                }))
        };
        let shortcut = if cfg!(target_os = "macos") {
            "⌘↩"
        } else {
            "Ctrl+Enter"
        };
        Some(
            div()
                .id("math-dialog-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("math-dialog")
                        .w(tokens::DIALOG_WIDTH + tokens::PLACEHOLDER_WIDTH / 2.)
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
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                                    if editing {
                                        "Edit formula"
                                    } else {
                                        "Insert formula"
                                    },
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .gap_0p5()
                                        .child(mode("math-inline", "Inline", false))
                                        .child(mode("math-display", "Display", true)),
                                ),
                        )
                        .child(
                            div()
                                .min_h(tokens::PREVIEW_HEIGHT / 2.)
                                .max_h(tokens::PREVIEW_HEIGHT * 2.)
                                .w_full()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.background)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .child(preview),
                        )
                        .child(
                            div()
                                .h(px(lines as f32 * 21. + 14.))
                                .rounded_md()
                                .border_1()
                                .border_color(theme.input)
                                .px_1()
                                .py_1()
                                .child(
                                    Editor::new(&dialog.editor)
                                        .bordered(false)
                                        .size_full()
                                        .font_family(theme.mono_font_family.clone())
                                        .text_sm(),
                                ),
                        )
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            if display {
                                "Saved as $$ … $$ on its own lines."
                            } else {
                                "Saved as $ … $ within the text."
                            },
                        ))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(Button::new("math-cancel").small().label("Cancel").on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.close_math_dialog(window, cx)
                                    }),
                                ))
                                .child(
                                    Button::new("math-confirm")
                                        .small()
                                        .primary()
                                        .icon(IconName::Check)
                                        .label(format!(
                                            "{} ({shortcut})",
                                            if editing { "Save" } else { "Insert" }
                                        ))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.confirm_math_dialog(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
