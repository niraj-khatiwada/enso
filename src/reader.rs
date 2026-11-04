//! Bounded Markdown parsing and layout for large documents. Only visible sections
//! are parsed. The outer variable-height list owns scrolling, never the document.
use gpui_kit::component::text::TextView;
use gpui_kit::{prelude::FluentBuilder, *};
use std::{ops::Range, path::PathBuf, sync::Arc};

const SECTION_BYTES: usize = 2048;
pub const LARGE_DOCUMENT_BYTES: usize = 256 * 1024;
/// Where text starts in the reading view, matching where GPUI Kit's editor
/// draws it with line numbers off (6 px padding + 6 px gutter margin across,
/// 8 px padding down), so switching between Read and Edit doesn't move it.
pub const TEXT_INSET_X: f32 = 12.;
pub const TEXT_INSET_Y: f32 = 8.;

/// Above this the editor stops soft-wrapping: wrapping measures every line up
/// front, which stalls on files this big. Below it, long lines wrap like the
/// reading view does.
pub const NO_WRAP_BYTES: usize = 4 * 1024 * 1024;

pub struct Section {
    range: Range<usize>,
    prefix: String,
    suffix: String,
}

pub struct LargeReader {
    source: Arc<String>,
    sections: Arc<Vec<Section>>,
    list: ListState,
    base: Option<PathBuf>,
    cursor_anchor: Option<usize>,
    controls: Option<(WeakEntity<crate::Enso>, EntityId)>,
    /// A highlighted match (a search jump, Find): its section is drawn from a
    /// state of its own, which can carry highlights; the others stay plain.
    highlight: Option<Highlight>,
}

struct Highlight {
    section: usize,
    /// In the section's own text (prefix included).
    ranges: Vec<Range<usize>>,
    color: Hsla,
    state: Entity<gpui_kit::component::text::TextViewState>,
    applied: bool,
}

impl LargeReader {
    /// Indexing runs on the file-loading worker; no Markdown AST is constructed.
    pub fn index(source: &str) -> Vec<Section> {
        let mut sections = Vec::new();
        let mut start = 0;
        let mut offset = 0;
        let mut fence: Option<(char, usize, String)> = None;
        let mut prefix = String::new();
        for line in source.split_inclusive('\n') {
            // Flush before the next line exceeds the synchronous parsing budget.
            // Prefer paragraph boundaries below, but never require an enormous
            // paragraph or a single long line to be laid out as one element.
            if offset > start && offset - start + line.len() > SECTION_BYTES {
                let suffix = fence
                    .as_ref()
                    .map(|(c, n, _)| format!("\n{}\n", c.to_string().repeat(*n)))
                    .unwrap_or_default();
                sections.push(Section {
                    range: start..offset,
                    prefix,
                    suffix,
                });
                prefix = fence
                    .as_ref()
                    .map(|(_, _, opening)| format!("{opening}\n"))
                    .unwrap_or_default();
                start = offset;
            }
            let trimmed = line.trim_start_matches(' ');
            if line.len() - trimmed.len() <= 3 {
                let marker = trimmed.chars().next().unwrap_or(' ');
                if marker == '`' || marker == '~' {
                    let count = trimmed.chars().take_while(|c| *c == marker).count();
                    if count >= 3 && count <= 128 {
                        match &fence {
                            Some((open, length, _))
                                if *open == marker
                                    && count >= *length
                                    && trimmed[count..].trim().is_empty() =>
                            {
                                fence = None
                            }
                            None => {
                                fence = Some((
                                    marker,
                                    count,
                                    trimmed.trim_end().chars().take(256).collect(),
                                ))
                            }
                            _ => {}
                        }
                    }
                }
            }
            let end = offset + line.len();
            while end - start > SECTION_BYTES {
                let mut boundary = start + SECTION_BYTES;
                while !source.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                let suffix = fence
                    .as_ref()
                    .map(|(c, n, _)| format!("\n{}\n", c.to_string().repeat(*n)))
                    .unwrap_or_default();
                sections.push(Section {
                    range: start..boundary,
                    prefix,
                    suffix,
                });
                prefix = fence
                    .as_ref()
                    .map(|(_, _, opening)| format!("{opening}\n"))
                    .unwrap_or_default();
                start = boundary;
            }
            offset = end;
            if offset - start >= SECTION_BYTES / 2 && line.trim().is_empty() && fence.is_none() {
                sections.push(Section {
                    range: start..offset,
                    prefix,
                    suffix: String::new(),
                });
                prefix = String::new();
                start = offset;
            }
        }
        if start < source.len() {
            sections.push(Section {
                range: start..source.len(),
                prefix,
                suffix: String::new(),
            });
        }
        sections
    }

    pub fn reveal_source(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.cursor_anchor = Some(offset);
        if self.sections.is_empty() {
            return;
        }
        let index = self
            .sections
            .partition_point(|section| section.range.end <= offset)
            .min(self.sections.len() - 1);
        self.list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    pub fn source(&self) -> Arc<String> {
        self.source.clone()
    }

    /// Highlight `ranges` of the source in `color` (those in the first one's
    /// section) and scroll to the first.
    pub fn highlight(&mut self, ranges: Vec<Range<usize>>, color: Hsla, cx: &mut Context<Self>) {
        let Some(first) = ranges.first().cloned() else { return };
        self.reveal_source(first.start, cx);
        let Some(index) = self.sections.iter().position(|s| s.range.contains(&first.start)) else {
            return;
        };
        let section = &self.sections[index];
        let local = ranges
            .iter()
            .filter(|r| section.range.contains(&r.start))
            .map(|r| {
                let end = r.end.min(section.range.end);
                section.prefix.len() + r.start - section.range.start..section.prefix.len() + end - section.range.start
            })
            .collect();
        let text = format!("{}{}{}", section.prefix, &self.source[section.range.clone()], section.suffix);
        let state = cx.new(|cx| gpui_kit::component::text::TextViewState::markdown(&text, cx));
        self.highlight = Some(Highlight { section: index, ranges: local, color, state, applied: false });
        cx.notify();
    }

    pub fn clear_highlight(&mut self, cx: &mut Context<Self>) {
        if self.highlight.take().is_some() {
            cx.notify();
        }
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub fn highlight_painted(&self) -> bool {
        self.highlight.as_ref().is_some_and(|h| h.applied)
    }

    #[cfg(all(test, feature = "ui-tests"))]
    pub fn visible_source_range(&self) -> Option<Range<usize>> {
        self.sections
            .get(self.list.logical_scroll_top().item_ix)
            .map(|section| section.range.clone())
    }

    pub fn refresh_layout(&mut self, cx: &mut Context<Self>) {
        self.list.remeasure();
        cx.notify();
    }

    pub fn replace(&mut self, source: Arc<String>, sections: Vec<Section>, cx: &mut Context<Self>) {
        let offset = self.list.logical_scroll_top();
        self.list
            .reset_with_uniform_height(sections.len() + 1, px(900.));
        self.list.scroll_to(offset);
        self.source = source;
        self.sections = Arc::new(sections);
        if let Some(offset) = self.cursor_anchor {
            self.reveal_source(offset, cx);
        }
        cx.notify();
    }

    pub fn set_base(&mut self, base: Option<PathBuf>, cx: &mut Context<Self>) {
        self.base = base; self.list.remeasure(); cx.notify();
    }
    pub fn set_controls(&mut self, owner: WeakEntity<crate::Enso>, editor: EntityId) {
        self.controls = Some((owner, editor));
    }

    pub fn new(source: Arc<String>, sections: Vec<Section>, base: Option<PathBuf>) -> Self {
        let list = ListState::new(sections.len() + 1, ListAlignment::Top, px(400.))
            .with_uniform_item_height(px(900.));
        Self {
            source,
            sections: Arc::new(sections),
            list,
            base,
            cursor_anchor: None,
            controls: None,
            highlight: None,
        }
    }
}

impl Render for LargeReader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Paint the highlight once its section's text has been parsed.
        if let Some(highlight) = self.highlight.as_mut().filter(|h| !h.applied) {
            let rendered = highlight.state.read(cx).rendered_text();
            if rendered.source().is_empty() {
                window.request_animation_frame();
            } else {
                highlight.applied = true;
                let color = highlight.color;
                let painted: Vec<_> = highlight
                    .ranges
                    .iter()
                    .filter_map(|r| rendered.range_for_source(r.clone()))
                    .map(|r| gpui_kit::component::text::RangeHighlight::new(r, color))
                    .collect();
                highlight.state.update(cx, |state, cx| {
                    let _ = state.set_range_highlights(painted, cx);
                });
            }
        }
        let highlighted = self.highlight.as_ref().map(|h| (h.section, h.state.clone()));
        let source = self.source.clone();
        let sections = self.sections.clone();
        let base = self.base.clone();
        let controls = self.controls.clone();
        let last = sections.len().saturating_sub(1);
        let content = list(self.list.clone(), move |index, _, cx| {
            if index == sections.len() {
                return div().pl(px(TEXT_INSET_X)).pr_4().child(
                    controls.as_ref().and_then(|(owner, editor)| owner.update(cx, |app, cx| app.linked_from(*editor, cx)).ok())
                        .unwrap_or_else(|| div().into_any_element())
                ).into_any_element();
            }
            let section = &sections[index];
            let text = format!(
                "{}{}{}",
                section.prefix,
                &source[section.range.clone()],
                section.suffix
            );
            let image_base = base.clone();
            let link_controls = controls.clone();
            div()
                .w_full()
                .min_h(px(24.))
                .pl(px(TEXT_INSET_X))
                // Clear of the scrollbar at the pane's right edge.
                .pr_4()
                // Breathing room above the first section and below the last;
                // part of the list, so it scrolls away with the text.
                .when(index == 0, |d| d.pt(px(TEXT_INSET_Y)))
                .when(index == last, |d| d.pb_8())
                .child(
                    div().w_full().child(
                        match &highlighted {
                            Some((section, state)) if *section == index => TextView::new(state),
                            _ => TextView::markdown(("section", index), text),
                        }
                            .map(|view| crate::typography::style(view, cx))
                            .map(|view| {
                                if let Some((owner, editor)) = &controls {
                                    crate::rich::decorate(
                                        view,
                                        owner.clone(),
                                        *editor,
                                        base.clone(),
                                        section.range.start,
                                        section.prefix.len(),
                                    )
                                } else {
                                    view
                                }
                            })
                            .selectable(true)
                            .w_full()
                            .image_source(move |uri| {
                                let source = uri.to_string();
                                if !source.contains(':') {
                                    if let Some(base) = &image_base {
                                        return ImageSource::from(base.join(crate::links::decode(&source)));
                                    }
                                }
                                ImageSource::from(uri.clone())
                            })
                            .on_link_click(move |url, event, window, cx| {
                                if !event.is_right_click() {
                                    if let Some((owner, editor)) = &link_controls {
                                        let _ = owner.update(cx, |app, cx| app.follow_note_link(*editor, url, window, cx));
                                    }
                                }
                            }),
                    ),
                )
                .into_any_element()
        })
        .size_full();
        div().relative().size_full().child(content).child(
            div()
                .absolute()
                .inset_0()
                .child(gpui_kit::base::Scrollbar::vertical(&self.list).viewport_from_layout()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn index_is_lossless_and_preserves_fence_continuations() {
        let source = format!(
            "# Title\n\n{}\n```rust\n{}\n```\n\nEnd",
            "A paragraph.\n\n".repeat(1000),
            "let x = 1;\n".repeat(2000)
        );
        let sections = LargeReader::index(&source);
        assert!(sections.len() > 5);
        let restored: String = sections.iter().map(|s| &source[s.range.clone()]).collect();
        assert_eq!(restored, source);
        assert!(sections.iter().any(|s| s.prefix == "```rust\n"));
        assert!(sections.iter().any(|s| s.suffix == "\n```\n"));
        for section in sections {
            assert!(section.range.len() < SECTION_BYTES * 3);
        }
    }
    #[::core::prelude::v1::test]
    fn huge_unicode_lines_stay_bounded_and_lossless() {
        let source = "नमस्ते 🟠".repeat(100_000);
        let sections = LargeReader::index(&source);
        let restored: String = sections.iter().map(|s| &source[s.range.clone()]).collect();
        assert_eq!(restored, source);
        assert!(sections.iter().all(|s| s.range.len() <= SECTION_BYTES));
    }

    #[::core::prelude::v1::test]
    #[ignore = "set ENSO_BENCH_FILE to a large UTF-8 fixture"]
    fn profile_large_file() {
        let path = std::env::var("ENSO_BENCH_FILE").expect("ENSO_BENCH_FILE");
        let began = std::time::Instant::now();
        let source = crate::files::read_text(std::path::Path::new(&path)).unwrap();
        let read = began.elapsed();
        let sections = LargeReader::index(&source);
        let total = began.elapsed();
        assert!(
            sections
                .iter()
                .all(|s| s.range.len() + s.prefix.len() + s.suffix.len() <= 4096)
        );
        let restored: String = sections.iter().map(|s| &source[s.range.clone()]).collect();
        assert_eq!(restored, source);
        eprintln!(
            "{} bytes; {} sections; read {:?}; read + index {:?}",
            source.len(),
            sections.len(),
            read,
            total
        );
    }
}
