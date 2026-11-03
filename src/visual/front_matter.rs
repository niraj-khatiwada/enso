use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, EditorState, InputEvent},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::document::Kind;
use super::view::{BODY_SIZE, VisualView};
use crate::tokens;

const LINE: f32 = 21.;

pub struct FrontMatterPanel {
    pub editor: Entity<EditorState>,
    pub lang: &'static str,
    pub collapsed: bool,
    _subscription: Subscription,
}

pub fn front_matter_lang(kind: &Kind, start: usize) -> Option<&'static str> {
    match kind {
        Kind::Fence {
            ch: b'-',
            closed: true,
            ..
        } if start == 0 => Some("yaml"),
        Kind::Fence {
            ch: b'+',
            closed: true,
            ..
        } if start == 0 => Some("toml"),
        _ => None,
    }
}

impl VisualView {
    pub fn front_matter_body(&self, cx: &App) -> Option<(Range<usize>, &'static str, String)> {
        let block = self.doc.blocks.first()?;
        let lang = front_matter_lang(&block.kind, block.start)?;
        let Kind::Fence { body, .. } = &block.kind else {
            return None;
        };
        let range = block.start + body.start..block.start + body.end;
        let text = self.editor.read(cx).text();
        if range.end > text.len() {
            return None;
        }
        Some((range.clone(), lang, text.slice(range).to_string()))
    }

    fn panel_text(body: &str) -> String {
        body.strip_suffix("\r\n")
            .or_else(|| body.strip_suffix('\n'))
            .unwrap_or(body)
            .replace("\r\n", "\n")
    }

    pub fn sync_front_matter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, lang, body)) = self.front_matter_body(cx) else {
            self.front_matter = None;
            return;
        };
        let shown = Self::panel_text(&body);
        if let Some(panel) = &self.front_matter
            && panel.lang == lang
        {
            let editor = panel.editor.clone();
            let focused = editor.read(cx).focus_handle(cx).is_focused(window);
            if !focused && editor.read(cx).value().as_ref() != shown.as_str() {
                editor.update(cx, |state, cx| state.set_value(shown, window, cx));
            }
            return;
        }
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(lang)
                .folding(false)
                .line_number(false)
                .default_value(shown)
        });
        editor.update(cx, |state, cx| state.set_soft_wrap(true, window, cx));
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = editor.read(cx).value().to_string();
                    this.write_front_matter(&value, window, cx);
                }
            },
        );
        self.front_matter = Some(FrontMatterPanel {
            editor,
            lang,
            collapsed: false,
            _subscription: subscription,
        });
    }

    fn write_front_matter(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((range, _, body)) = self.front_matter_body(cx) else {
            return;
        };
        let crlf = body.contains("\r\n");
        let mut text = if crlf {
            value.replace('\n', "\r\n")
        } else {
            value.to_string()
        };
        if !text.is_empty() || !body.is_empty() {
            text.push_str(if crlf { "\r\n" } else { "\n" });
        }
        if text == body {
            return;
        }
        let caret = self.caret;
        let delta = text.len() as isize - range.len() as isize;
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(range.clone(), cx);
            state.replace(text, window, cx);
        });
        self.sync(cx);
        if caret >= range.end {
            self.caret = (caret as isize + delta).max(0) as usize;
        }
        self.caret = self
            .doc
            .snap(self.caret.min(self.editor.read(cx).text().len()));
        cx.notify();
    }

    pub fn render_front_matter(&self, pad_top: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let Some(panel) = &self.front_matter else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let lines = panel.editor.read(cx).value().lines().count().max(1);
        let collapsed = panel.collapsed;
        let label = match panel.lang {
            "toml" => "TOML",
            _ => "YAML",
        };
        div()
            .w_full()
            .pt(pad_top)
            .pb(px(BODY_SIZE * 1.2))
            .child(
                div()
                    .id("front-matter")
                    .w_full()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary.opacity(tokens::FADED))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_3()
                            .h(tokens::CONTROL_LG)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(IconName::FileText)
                                    .child("Front matter")
                                    .child(
                                        div().px_1().rounded_sm().bg(theme.secondary).child(label),
                                    ),
                            )
                            .child(
                                Button::new("front-matter-toggle")
                                    .ghost()
                                    .xsmall()
                                    .icon(if collapsed {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronUp
                                    })
                                    .tooltip(if collapsed { "Show" } else { "Hide" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(panel) = &mut this.front_matter {
                                            panel.collapsed = !panel.collapsed;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    )
                    .when(!collapsed, |card| {
                        card.child(
                            div()
                                .border_t_1()
                                .border_color(theme.border)
                                .px_1()
                                .py_1()
                                .h(px(lines as f32 * LINE + 22.))
                                .child(
                                    Editor::new(&panel.editor)
                                        .bordered(false)
                                        .size_full()
                                        .font_family(theme.mono_font_family.clone())
                                        .text_sm(),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }
}
