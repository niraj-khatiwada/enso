use gpui_kit::{base::StyledExt, *};

use crate::{
    chrome,
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

fn entry_label(element: &crate::core::element::Element) -> String {
    match element {
        crate::core::element::Element::Text(t) => {
            let s = t.text.clone();
            if s.chars().count() > 14 {
                s.chars().take(14).collect::<String>() + "…"
            } else {
                s
            }
        }
        _ => format!("{:?}", element.kind()),
    }
}

pub fn render_library_panel(editor: &mut Editor, cx: &mut Context<Editor>) -> gpui_kit::Div {
    let cx = &*cx;
    let tokens = tokens_of(editor);

    let items: Vec<(String, String)> = editor
        .document
        .library
        .iter()
        .map(|e| (e.id().to_string(), entry_label(e)))
        .collect();

    let mut panel = div()
        .w(px(size::PANEL_WIDTH))
        .flex()
        .flex_col()
        .gap(px(6.0))
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
                .child(SharedString::from(editor.i18n.t("panel.library"))),
        )
        .child(
            chrome::menu_row(
                "lib-add",
                Some("PlusIcon"),
                &editor.i18n.t("library.addSelected"),
                tokens,
            )
            .on_click(Editor::act(cx, |this, _, _, _| {
                this.document.add_selected_to_library();
            })),
        );

    if items.is_empty() {
        panel = panel.child(
            div()
                .p(px(4.0))
                .text_size(px(12.0))
                .text_color(tokens.surface_text().alpha(0.55))
                .child(SharedString::from(editor.i18n.t("library.empty"))),
        );
    } else {
        let rows = items.into_iter().map(move |(id, name)| {
            let id_insert = id.clone();
            let id_remove = id.clone();
            let fg = tokens.surface_text();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .id(format!("lib-insert-{id_insert}"))
                        .flex_1()
                        .min_h(px(size::MENU_ROW))
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .rounded(px(size::RADIUS_MD))
                        .text_size(px(13.0))
                        .text_color(fg)
                        .hover(move |s| s.bg(tokens.hover()))
                        .child(SharedString::from(name))
                        .on_click(Editor::act(cx, move |this, _, _, _| {
                            if let Some(new_id) = this.document.insert_from_library(&id_insert) {
                                this.document.select(&new_id);
                            }
                        })),
                )
                .child(
                    div()
                        .id(format!("lib-del-{id_remove}"))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(size::BUTTON))
                        .rounded(px(size::RADIUS_MD))
                        .text_color(fg)
                        .child(icon("TrashIcon", size::ICON, fg))
                        .hover(move |s| s.bg(tokens.hover()))
                        .on_click(Editor::act(cx, move |this, _, _, _| {
                            this.document.remove_from_library(&id_remove);
                        })),
                )
        });
        panel = panel.child(div().flex().flex_col().gap(px(2.0)).children(rows));
    }

    panel
}
