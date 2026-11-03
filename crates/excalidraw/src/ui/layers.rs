use gpui_kit::{base::StyledExt, *};

use crate::{
    design::{Tokens, island_shadow, size},
    editor::Editor,
    icons::icon,
};

fn tokens_of(editor: &Editor) -> Tokens {
    let palette = match editor.document.theme {
        crate::core::types::Theme::Dark => crate::design::Palette::Dark,
        crate::core::types::Theme::Light => crate::design::Palette::Light,
    };
    Tokens::for_palette(palette)
}

fn name_of(element: &crate::core::element::Element) -> String {
    match element {
        crate::core::element::Element::Text(t) => {
            let s = t.text.clone();
            if s.chars().count() > 12 {
                s.chars().take(12).collect::<String>() + "…"
            } else {
                s
            }
        }
        _ => format!("{:?}", element.kind()),
    }
}

fn row_action(id: String, icon_name: &str, tokens: Tokens) -> Stateful<Div> {
    let fg = tokens.surface_text();
    div()
        .id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size::BUTTON))
        .rounded(px(size::RADIUS_MD))
        .text_color(fg)
        .child(icon(icon_name, size::ICON, fg))
        .hover(move |s| s.bg(tokens.hover()))
}

pub fn render_layers_panel(editor: &mut Editor, cx: &mut Context<Editor>) -> gpui_kit::Div {
    let cx = &*cx;
    let tokens = tokens_of(editor);

    let layers: Vec<(String, String, bool)> = editor
        .document
        .scene
        .non_deleted()
        .map(|e| (e.id().to_string(), name_of(e), e.base().locked))
        .collect();

    let selected_ids = editor.document.selected.clone();

    let panel = div()
        .w(px(size::PANEL_WIDTH))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(12.0))
        .overflow_hidden()
        .bg(tokens.island())
        .rounded(px(size::RADIUS_LG))
        .shadow(island_shadow())
        .child(
            div()
                .text_size(px(13.0))
                .font_bold()
                .text_color(tokens.surface_text())
                .child(SharedString::from(editor.i18n.t("panel.layers"))),
        );

    if layers.is_empty() {
        return panel.child(
            div()
                .p(px(4.0))
                .text_size(px(12.0))
                .text_color(tokens.surface_text().alpha(0.55))
                .child(SharedString::from(editor.i18n.t("library.empty"))),
        );
    }

    let rows = layers
        .into_iter()
        .enumerate()
        .map(move |(idx, (id, name, locked))| {
            let id_select = id.clone();
            let id_up = id.clone();
            let id_down = id.clone();
            let id_del = id.clone();
            let is_selected = selected_ids.contains(&id);
            let fg = tokens.surface_text();

            let label = div()
                .id(format!("layer-{id}"))
                .flex_1()
                .min_h(px(size::MENU_ROW))
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .rounded(px(size::RADIUS_MD))
                .text_size(px(13.0))
                .text_color(fg)
                .hover(move |s| s.bg(tokens.hover()))
                .child(SharedString::from(format!("{}. {name}", idx + 1)))
                .on_click(Editor::act(cx, move |this, _, _, _| {
                    this.document.select(&id_select);
                }));
            let label = if locked {
                label.child(icon("LockedIcon", size::ICON, fg))
            } else {
                label
            };

            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .rounded(px(size::RADIUS_MD))
                .child(label)
                .child(
                    row_action(format!("layer-up-{id_up}"), "BringForwardIcon", tokens).on_click(
                        Editor::act(cx, move |this, _, _, _| {
                            this.push_checkpoint();
                            this.document.select(&id_up);
                            this.document.bring_forward();
                        }),
                    ),
                )
                .child(
                    row_action(format!("layer-down-{id_down}"), "SendBackwardIcon", tokens)
                        .on_click(Editor::act(cx, move |this, _, _, _| {
                            this.push_checkpoint();
                            this.document.select(&id_down);
                            this.document.send_backward();
                        })),
                )
                .child(
                    row_action(format!("layer-del-{id_del}"), "TrashIcon", tokens).on_click(
                        Editor::act(cx, move |this, _, _, _| {
                            this.push_checkpoint();
                            this.document.select(&id_del);
                            this.document.delete_selected();
                        }),
                    ),
                );

            if is_selected {
                row = row.bg(tokens.selected_bg().alpha(0.35));
            }
            row.into_any_element()
        });

    panel.child(div().flex().flex_col().gap(px(2.0)).children(rows))
}
