//! A small heading rail shared by the source editor and reading view.
use super::*;
/// Width of the heading rail beside the document.
const RAIL_WIDTH: f32 = 28.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub title: String,
    pub level: usize,
    pub offset: usize,
    pub preview: String,
    /// The same section as read, without Markdown, for Live Preview.
    pub plain: String,
}
pub fn headings(source: &str) -> Vec<Heading> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut previous: Option<(usize, String)> = None;
    for line in source.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\r', '\n']);
        let text = raw.trim_start_matches(' ');
        let indent = raw.len() - text.len();
        let marker = text.as_bytes().first().copied().unwrap_or(0);
        let count = text.bytes().take_while(|b| *b == marker).count();
        if indent <= 3 && matches!(marker, b'`' | b'~') && count >= 3 {
            match fence {
                Some((ch, n)) if ch == marker && count >= n && text[count..].trim().is_empty() => {
                    fence = None
                }
                None => fence = Some((marker, count)),
                _ => {}
            }
            previous = None;
        } else if fence.is_none() && indent <= 3 {
            let level = text.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&level)
                && (text.len() == level || text.as_bytes()[level].is_ascii_whitespace())
            {
                let title = text[level..].trim().trim_end_matches('#').trim().to_owned();
                if !title.is_empty() {
                    result.push(Heading {
                        title,
                        level,
                        offset,
                        preview: String::new(),
                        plain: String::new(),
                    });
                }
                previous = None;
            } else if !text.is_empty()
                && matches!(marker, b'=' | b'-')
                && text.bytes().all(|b| b == marker || b.is_ascii_whitespace())
            {
                if let Some((start, title)) = previous.take() {
                    result.push(Heading {
                        title,
                        level: if marker == b'=' { 1 } else { 2 },
                        offset: start,
                        preview: String::new(),
                        plain: String::new(),
                    });
                }
            } else if text.is_empty() {
                previous = None;
            } else {
                previous = Some((offset, text.to_owned()));
            }
        } else {
            previous = None;
        }
        offset += line.len();
    }
    for i in 0..result.len() {
        let end = result.get(i + 1).map_or(source.len(), |h| h.offset);
        let body = &source[result[i].offset..end];
        let body = body.split_once('\n').map_or("", |(_, rest)| rest);
        result[i].plain = plain_text(body).chars().take(180).collect();
        result[i].preview = source[result[i].offset..end]
            .lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .take(4)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(180)
            .collect();
    }
    result
}
/// Markdown as read: the words of its paragraphs, lists and tables (cells
/// joined by " · "), leaving out code, diagrams and HTML.
pub fn plain_text(markdown: &str) -> String {
    use markdown::mdast::Node;
    fn words(node: &Node, out: &mut String) {
        match node {
            Node::Text(t) => out.push_str(&t.value.replace('\n', " ")),
            Node::InlineCode(c) => out.push_str(&c.value),
            Node::Break(_) => out.push(' '),
            Node::Code(_) | Node::Html(_) | Node::Math(_) | Node::Image(_) | Node::Definition(_) => {}
            Node::TableCell(_) => {
                for child in node.children().into_iter().flatten() {
                    words(child, out);
                }
                out.push_str(" · ");
            }
            _ => {
                let block = matches!(node, Node::Paragraph(_) | Node::Heading(_) | Node::ListItem(_) | Node::TableRow(_));
                for child in node.children().into_iter().flatten() {
                    words(child, out);
                }
                if block {
                    out.push(' ');
                }
            }
        }
    }
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let Ok(tree) = markdown::to_mdast(markdown, &options) else { return String::new() };
    let mut out = String::new();
    words(&tree, &mut out);
    let text = out.split_whitespace().collect::<Vec<_>>().join(" ");
    text.trim_end_matches('·').trim().to_string()
}

impl Enso {
    pub fn outline_rail(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !settings::document_outline(cx) {
            return div().into_any_element();
        }
        let live = self.live_view(cx);
        let Some(doc) = self
            .active
            .and_then(|i| self.documents.get(i))
            .filter(|d| d.markdown)
        else {
            return div().into_any_element();
        };
        let id = doc.editor.entity_id();
        let theme = cx.theme().clone();
        let pointer = self
            .outline_hover
            .filter(|(editor, _)| {
                *editor == id
                    && self.palette.is_none()
                    && self.closing_palette.is_none()
                    && self.controls.is_none()
                    && self.closing_controls.is_none()
            })
            .map(|(_, p)| p);
        let bounds = self.outline_bounds.clone();
        let measure = bounds.clone();
        let pointer_bounds = bounds.clone();
        let count = doc.outline.len().min(120);
        // Blend into the pane beside the rail: the editor paints its own solid
        // background, the reader shows the window's.
        let beside_editor = self.mode == Mode::Edit
            || (self.mode == Mode::Split && !settings::reader_first(cx));
        let editor_background = theme
            .highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| theme.input_background());
        let mut rail = v_flex()
            .id("outline-rail")
            .debug_selector(|| "outline-rail".into())
            .relative()
            .w(px(RAIL_WIDTH))
            .h(px(count as f32 * 8. + 24.))
            .max_h(relative(0.5))
            .flex_shrink_0()
            .py_3()
            .pl(px(8.))
            .cursor_pointer()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| {
                        *measure.lock().unwrap() = bounds;
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_move(cx.listener(move |app, event: &MouseMoveEvent, _, cx| {
                let b = *pointer_bounds.lock().unwrap();
                let p = (f32::from(event.position.y - b.top() - px(12.))
                    / f32::from((b.size.height - px(24.)).max(px(1.))))
                .clamp(0., 0.99999);
                if app.outline_hover.map_or(true, |(editor, old)| {
                    editor != id || (p - old).abs() > 0.0002
                }) {
                    app.outline_hover = Some((id, p));
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                if !*hovered {
                    app.outline_hover = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |app, _, w, cx| {
                if let Some((editor, p)) = app.outline_hover.filter(|(editor, _)| *editor == id) {
                    let offset = app
                        .documents
                        .iter()
                        .find(|d| d.editor.entity_id() == editor)
                        .and_then(|d| d.outline.get((p * d.outline.len() as f32) as usize))
                        .map(|h| h.offset);
                    if let Some(offset) = offset {
                        app.jump_to_heading(editor, offset, w, cx);
                    }
                }
            }));
        for i in 0..count {
            let distance = pointer.map(|p| ((i as f32 + 0.5) - p * count as f32).abs());
            let influence = distance.map_or(0., |d| (-0.5 * (d / 1.65).powi(2)).exp());
            let selected = pointer.is_some_and(|p| (p * count as f32) as usize == i);
            let width = transition(
                ("outline-tick-width", i.to_string()),
                4. + 14. * influence,
                Transition::new(std::time::Duration::from_millis(100)),
                window,
                cx,
            );
            rail = rail.child(
                div()
                    .flex_1()
                    .min_h(px(1.))
                    .w_full()
                    .flex()
                    .items_center()
                    .child(div().h(px(2.)).w(px(width)).rounded_full().bg(
                        theme.foreground.opacity(if selected {
                            0.95
                        } else {
                            0.20 + 0.28 * influence
                        }),
                    )),
            );
        }
        if let Some(p) = pointer {
            if let Some(heading) = doc.outline.get((p * doc.outline.len() as f32) as usize) {
                let height = f32::from(bounds.lock().unwrap().size.height);
                let top = (12. + p * (height - 24.) - 36.).clamp(8., (height - 140.).max(8.));
                rail = rail.child(
                    deferred(
                        v_flex()
                            .absolute()
                            .left(px(RAIL_WIDTH + 2.))
                            .top(px(top))
                            .w(px(300.))
                            .gap_2()
                            .p_3()
                            .card_surface(&theme)
                            .occlude()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(heading.title.clone()),
                            )
                            .map(|card| {
                                // Live Preview never shows Markdown: the text as read.
                                let preview = if live { heading.plain.clone() } else { heading.preview.clone() };
                                card.when(!preview.is_empty(), |card| {
                                    card.child(div().text_sm().text_color(theme.muted_foreground).child(preview))
                                })
                            }),
                    )
                    .with_priority(1),
                );
            }
        }
        // The background spans the column's full height, not just the rail,
        // so the strip above and below it blends in too.
        v_flex()
            .h_full()
            .w(px(RAIL_WIDTH))
            .flex_shrink_0()
            .justify_center()
            .when(beside_editor, |column| column.bg(editor_background))
            .child(rail)
            .into_any_element()
    }
    pub(super) fn jump_to_heading(
        &mut self,
        id: EntityId,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .documents
            .iter()
            .position(|d| d.editor.entity_id() == id)
        else {
            return;
        };
        let doc = &self.documents[index];
        // The cursor moves in Read too, so switching to Edit lands there.
        if doc.editor_loaded {
            doc.editor.update(cx, |e, cx| {
                e.set_selected_range(offset..offset, cx);
            });
        }
        // Live Preview: the caret on the heading in the page, in view.
        if let Some(live) = doc.live.clone().filter(|_| self.mode != Mode::Edit) {
            live.update(cx, |live, cx| live.jump_to(offset, window, cx));
            cx.notify();
            return;
        }
        if self.mode != Mode::Read {
            doc.editor.focus_handle(cx).focus(window, cx);
        }
        if self.mode != Mode::Edit {
            if let Some(reader) = &doc.large_reader {
                reader.update(cx, |r, cx| r.reveal_source(offset, cx));
            } else {
                let rendered = doc.preview.read(cx).rendered_text();
                let source = rendered.source();
                let end = source
                    .get(offset..)
                    .map(|tail| offset + tail.find('\n').unwrap_or(tail.len()));
                if let Some(range) = end.and_then(|end| rendered.range_for_source(offset..end)) {
                    doc.preview.update(cx, |v, cx| {
                        let _ = v.reveal_range(range, cx);
                    });
                }
            }
        }
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn previews_read_without_markdown() {
        let section = "| Area | Reviewer |\n| :--- | :--- |\n| Copy | **Leena** |\n\n- [ ] Pack `bags`\n\n```sh\ncargo run\n```\n";
        assert_eq!(super::plain_text(section), "Area · Reviewer · Copy · Leena · Pack bags");
    }

    #[test]
    fn headings_skip_code_and_keep_offsets() {
        let source = "# A\n\n```md\n# fake\n```\n\nSub\n---\n\n## B\n";
        let h = super::headings(source);
        assert_eq!(
            h.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(),
            vec!["A", "Sub", "B"]
        );
        assert_eq!(&source[h[2].offset..h[2].offset + 4], "## B");
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests {
    use crate::{Enso, Mode};
    use gpui_kit::{
        AppContext, Modifiers, TestAppContext, VisualTestContext, WindowOptions, point, px,
    };
    #[gpui_kit::test]
    fn rail_previews_follow_pointer_and_click_navigates(cx: &mut TestAppContext) {
        // The classic reader's own controls (large notes use it).
        crate::use_classic_reader_for_test();
        cx.update(gpui_kit::init);
        let source = (0..80)
            .map(|i| format!("## Section {i}\n\nParagraph for section {i}.\n\n"))
            .collect::<String>();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |w, cx| {
                cx.new(|cx| {
                    let mut app = Enso::new(w, cx);
                    app.add_document(None, source, None, w, cx);
                    app
                })
            })
            .unwrap()
        });
        for _ in 0..2 {
            cx.update_window(handle, |_, w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
        let mut visual = VisualTestContext::from_window(handle, cx);
        let bounds = visual.debug_bounds("outline-rail").unwrap();
        let viewport_height = cx
            .update_window(handle, |_, w, _| w.viewport_size().height)
            .unwrap();
        assert!(
            bounds.size.height <= viewport_height * 0.5,
            "outline must occupy at most half the document area"
        );
        let position = point(
            bounds.left() + px(14.),
            bounds.top() + px(12.) + (bounds.size.height - px(24.)) * 0.75,
        );
        visual.simulate_mouse_move(position, None, Modifiers::default());
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.update(|cx| {
            let app = view.read(cx);
            let (_, p) = app.outline_hover.unwrap();
            assert!((p - 0.75).abs() < 0.01);
            let heading = &app.documents[0].outline[(p * 80.) as usize];
            assert_eq!(heading.title, "Section 60");
            assert!(heading.preview.contains("Paragraph for section 60"));
        });
        visual.simulate_click(position, Modifiers::default());
        cx.update_window(handle, |_, w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            let app = view.read(cx);
            assert!(app.mode == Mode::Read);
            assert!(
                app.documents[0]
                    .preview
                    .read(cx)
                    .list_state()
                    .logical_scroll_top()
                    .item_ix
                    > 0
            );
        });
    }
}
