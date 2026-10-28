//! Bounded Markdown parsing and layout for large documents. Only visible sections
//! are parsed. The outer variable-height list owns scrolling, never the document.
use gpui_kit::component::text::TextView;
use gpui_kit::{prelude::FluentBuilder, *};
use std::{ops::Range, path::PathBuf, sync::Arc};

const SECTION_BYTES: usize = 2048;
pub const LARGE_DOCUMENT_BYTES: usize = 256 * 1024;
/// Where text starts in the reading view, matching where GPUI Kit's editor
/// draws it with line numbers off (6 px padding + 12 px gutter margin across,
/// 8 px padding down), so switching between Read and Edit doesn't move it.
pub const TEXT_INSET_X: f32 = 18.;
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
            .reset_with_uniform_height(sections.len(), px(900.));
        self.list.scroll_to(offset);
        self.source = source;
        self.sections = Arc::new(sections);
        if let Some(offset) = self.cursor_anchor {
            self.reveal_source(offset, cx);
        }
        cx.notify();
    }

    pub fn new(source: Arc<String>, sections: Vec<Section>, base: Option<PathBuf>) -> Self {
        let list = ListState::new(sections.len(), ListAlignment::Top, px(400.))
            .with_uniform_item_height(px(900.));
        Self {
            source,
            sections: Arc::new(sections),
            list,
            base,
            cursor_anchor: None,
        }
    }
}

impl Render for LargeReader {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let source = self.source.clone();
        let sections = self.sections.clone();
        let base = self.base.clone();
        let last = sections.len().saturating_sub(1);
        let content = list(self.list.clone(), move |index, _, _| {
            let section = &sections[index];
            let text = format!(
                "{}{}{}",
                section.prefix,
                &source[section.range.clone()],
                section.suffix
            );
            let image_base = base.clone();
            let link_base = base.clone();
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
                        TextView::markdown(("section", index), text)
                            .selectable(true)
                            .w_full()
                            .image_source(move |uri| {
                                let source = uri.to_string();
                                if !source.contains(':') {
                                    if let Some(base) = &image_base {
                                        return ImageSource::from(base.join(source));
                                    }
                                }
                                ImageSource::from(uri.clone())
                            })
                            .on_link_click(move |url, event, _, cx| {
                                if event.is_right_click() {
                                    return;
                                }
                                if url.starts_with("https://")
                                    || url.starts_with("http://")
                                    || url.starts_with("mailto:")
                                {
                                    cx.open_url(url);
                                } else if !url.contains(':') && !url.starts_with('#') {
                                    if let Some(base) = &link_base {
                                        cx.reveal_path(&base.join(url.as_ref()));
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
