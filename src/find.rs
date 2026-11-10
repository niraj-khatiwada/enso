//! Find in the reading view (Cmd/Ctrl+F in Read, or in Split's reading
//! side), and the brief highlight a search result's match gets when it's
//! opened. The editor keeps its own Find and Replace.

use super::*;
use gpui_kit::component::text::RangeHighlight;
use std::{ops::Range, time::Duration};

/// How long a jump's match stays highlighted.
const FLASH: Duration = Duration::from_secs(3);
/// More matches than this aren't counted or painted.
const MAX_MATCHES: usize = 10_000;

pub(crate) struct PageFind {
    pub(crate) editor_id: EntityId,
    pub(crate) input: Entity<InputState>,
    /// In the rendered text of an ordinary note, in the source of a large one.
    pub(crate) matches: Vec<Range<usize>>,
    pub(crate) current: usize,
    /// The text the matches are for.
    query: Option<String>,
    _subscription: Subscription,
}

/// Every match softly, the current one strongly.
pub(crate) fn colors(cx: &App) -> (Hsla, Hsla) {
    let dark = cx.theme().mode.is_dark();
    (
        hsla(0.14, 0.95, 0.55, if dark { 0.22 } else { 0.30 }),
        hsla(0.10, 1.0, 0.55, if dark { 0.55 } else { 0.60 }),
    )
}

impl Enso {
    /// The live page showing the note `id`, when Live Preview shows it.
    fn live_page(&self, id: EntityId, cx: &App) -> Option<Entity<crate::live::LiveView>> {
        let doc = self.documents.iter().find(|d| d.editor.entity_id() == id)?;
        let _ = cx;
        doc.live.clone().filter(|_| self.mode != Mode::Edit)
    }

    /// Cmd/Ctrl+F: in the editor its own Find opens (it binds the keys
    /// closer in); here the reading view gets one.
    pub(crate) fn open_page_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active else { return };
        if self.mode == Mode::Edit || !self.documents[index].markdown {
            cx.propagate();
            return;
        }
        let doc = &self.documents[index];
        let editor_id = doc.editor.entity_id();
        if let Some(find) = &self.page_find
            && find.editor_id == editor_id
        {
            let input = find.input.clone();
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
            return;
        }
        // What's selected in the reader is what to look for.
        let live = self.live_page(editor_id, cx);
        let selected = if let Some(live) = &live {
            live.read(cx).selected_text()
        } else if doc.large_reader.is_none() {
            doc.preview.read(cx).selected_text()
        } else {
            String::new()
        };
        let query = selected.lines().next().unwrap_or_default().trim().to_string();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find").default_value(query));
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
            InputEvent::Change => this.page_find_search(cx),
            InputEvent::PressEnter { shift, .. } => this.page_find_step(if *shift { -1 } else { 1 }, cx),
            _ => {}
        });
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.page_find = Some(PageFind {
            editor_id,
            input,
            matches: Vec::new(),
            current: 0,
            query: None,
            _subscription: subscription,
        });
        self.page_find_search(cx);
    }

    pub(crate) fn close_page_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.page_find.take() else { return };
        self.clear_reader_highlights(find.editor_id, cx);
        // Back to the page, so the keys move it again.
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Find again, after the query changed.
    fn page_find_search(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.page_find.as_ref() else { return };
        let query = find.input.read(cx).value().to_string();
        // The input reports a change on Enter too; only new text searches anew.
        if find.query.as_deref() == Some(query.as_str()) {
            return;
        }
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == find.editor_id) else {
            return;
        };
        let live = self.live_page(doc.editor.entity_id(), cx);
        let matches = match &doc.large_reader {
            // Live Preview: in the page's text (as the note's offsets).
            _ if live.is_some() => live.as_ref().map(|l| l.read(cx).find_all(&query, MAX_MATCHES)).unwrap_or_default(),
            // The editor of a large note loads only when it's opened.
            Some(reader) => search::find_all(&reader.read(cx).source(), &query, MAX_MATCHES),
            None => search::find_all(doc.preview.read(cx).rendered_text().as_str(), &query, MAX_MATCHES),
        };
        let find = self.page_find.as_mut().unwrap();
        find.query = Some(query);
        find.matches = matches;
        find.current = 0;
        self.show_page_find(cx);
    }

    /// The next match (`step` 1) or the previous one (-1), around the ends.
    pub(crate) fn page_find_step(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(find) = self.page_find.as_mut() else { return };
        let count = find.matches.len();
        if count == 0 {
            return;
        }
        find.current = (find.current as isize + step).rem_euclid(count as isize) as usize;
        self.show_page_find(cx);
    }

    /// Paint the matches and bring the current one into view.
    fn show_page_find(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.page_find.as_ref() else { return };
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == find.editor_id) else {
            return;
        };
        self.flash_task = None;
        if let Some(live) = self.live_page(find.editor_id, cx) {
            let (matches, current) = (find.matches.clone(), (!find.matches.is_empty()).then_some(find.current));
            live.update(cx, |live, cx| live.show_found(matches, current, cx));
            cx.notify();
            return;
        }
        let (soft, strong) = colors(cx);
        let current = find.matches.get(find.current).cloned();
        match &doc.large_reader {
            Some(reader) => reader.update(cx, |reader, cx| match current {
                Some(range) => reader.highlight(vec![range], strong, cx),
                None => reader.clear_highlight(cx),
            }),
            None => {
                let highlights = find
                    .matches
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != find.current)
                    .map(|(_, r)| RangeHighlight::new(r.clone(), soft))
                    // The current one last, painted over the rest.
                    .chain(current.clone().map(|r| RangeHighlight::new(r, strong)))
                    .collect::<Vec<_>>();
                doc.preview.update(cx, |view, cx| {
                    let _ = view.set_range_highlights(highlights, cx);
                    if let Some(range) = current {
                        let _ = view.reveal_range(range, cx);
                    }
                });
            }
        }
        cx.notify();
    }

    /// A search result opened at `ranges` (of the source: the phrase, or
    /// each word): the editor selects them, from the first to the last, and
    /// the reader highlights each for a few seconds.
    pub(crate) fn flash_match(&mut self, id: EntityId, ranges: Vec<Range<usize>>, cx: &mut Context<Self>) {
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == id) else { return };
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else { return };
        let span = first.start..last.end.max(first.end);
        if doc.editor_loaded {
            doc.editor.update(cx, |editor, cx| editor.set_selected_range(span, cx));
        }
        if (self.mode == Mode::Edit && self.live_page(id, cx).is_none()) || self.page_find.is_some() {
            return;
        }
        let (_, strong) = colors(cx);
        match &doc.large_reader {
            _ if self.live_page(id, cx).is_some() => {
                if let Some(live) = self.live_page(id, cx) {
                    let count = ranges.len();
                    live.update(cx, |live, cx| live.show_found(ranges, (count > 0).then_some(0), cx));
                }
            }
            Some(reader) => reader.update(cx, |reader, cx| reader.highlight(ranges, strong, cx)),
            None => {
                let rendered = doc.preview.read(cx).rendered_text();
                let painted: Vec<_> = ranges
                    .into_iter()
                    .filter_map(|r| rendered.range_for_source(r))
                    .map(|r| RangeHighlight::new(r, strong))
                    .collect();
                if painted.is_empty() {
                    return;
                }
                doc.preview.update(cx, |view, cx| {
                    let _ = view.set_range_highlights(painted, cx);
                });
            }
        }
        self.flash_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH).await;
            let _ = this.update(cx, |this, cx| {
                if this.page_find.is_none() {
                    this.clear_reader_highlights(id, cx);
                }
            });
        }));
    }

    fn clear_reader_highlights(&mut self, id: EntityId, cx: &mut Context<Self>) {
        if let Some(live) = self.live_page(id, cx) {
            live.update(cx, |live, cx| live.clear_found(cx));
        }
        let Some(doc) = self.documents.iter().find(|d| d.editor.entity_id() == id) else { return };
        match &doc.large_reader {
            Some(reader) => reader.update(cx, |reader, cx| reader.clear_highlight(cx)),
            None => doc.preview.update(cx, |view, cx| view.clear_range_highlights(cx)),
        }
    }

    /// The bar over the reading view: the query, where it is among the
    /// matches, previous and next, and close.
    pub(crate) fn page_find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.page_find.as_ref()?;
        let theme = cx.theme().clone();
        let empty = find.input.read(cx).value().is_empty();
        let count = match find.matches.len() {
            _ if empty => String::new(),
            0 => "No Matches".into(),
            n => format!("{}/{}", find.current + 1, n),
        };
        Some(
            h_flex()
                .id("page-find")
                .key_context("PageFind")
                .absolute()
                .top_2()
                .right_5()
                .h(px(34.))
                .pl_2p5()
                .pr_1()
                .gap_1()
                .items_center()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_md()
                .occlude()
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.close_page_find(window, cx);
                    }
                }))
                .child(Icon::new(IconName::Search).size_3p5().text_color(theme.muted_foreground))
                .child(div().w(px(180.)).child(Input::new(&find.input).xsmall().appearance(false).text_sm()))
                .child(
                    div()
                        .min_w(px(28.))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(count),
                )
                .child(
                    ui::icon_button("find-previous", IconName::ChevronUp, "Previous Match")
                        .xsmall()
                        .compact()
                        .on_click(cx.listener(|this, _, _, cx| this.page_find_step(-1, cx))),
                )
                .child(
                    ui::icon_button("find-next", IconName::ChevronDown, "Next Match")
                        .xsmall()
                        .compact()
                        .on_click(cx.listener(|this, _, _, cx| this.page_find_step(1, cx))),
                )
                .child(
                    ui::icon_button("find-close", IconName::Close, "Close")
                        .xsmall()
                        .compact()
                        .on_click(cx.listener(|this, _, window, cx| this.close_page_find(window, cx))),
                )
                .into_any_element(),
        )
    }
}
