use std::ops::Range;

use gpui_kit::base::input::RopeExt as _;
use gpui_kit::*;

use super::view::VisualView;

impl VisualView {
    fn bytes_to_utf16(&self, range: Range<usize>, cx: &App) -> Range<usize> {
        let rope = self.editor.read(cx).text();
        rope.offset_to_offset_utf16(range.start)..rope.offset_to_offset_utf16(range.end)
    }

    fn utf16_to_bytes(&self, range: Range<usize>, cx: &App) -> Range<usize> {
        let rope = self.editor.read(cx).text();
        rope.offset_utf16_to_offset(range.start)..rope.offset_utf16_to_offset(range.end)
    }

    fn input_range(&self, range_utf16: Option<Range<usize>>, cx: &App) -> Range<usize> {
        if let Some(marked) = &self.marked {
            return marked.clone();
        }
        range_utf16.map_or_else(|| self.selection(), |r| self.utf16_to_bytes(r, cx))
    }
}

impl EntityInputHandler for VisualView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.utf16_to_bytes(range_utf16, cx);
        *adjusted = Some(self.bytes_to_utf16(range.clone(), cx));
        Some(self.editor.read(cx).text().slice(range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.bytes_to_utf16(self.selection(), cx),
            reversed: self.anchor.is_some_and(|a| a > self.caret),
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|r| self.bytes_to_utf16(r, cx))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16, cx);
        self.marked = None;
        self.edit(range, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.input_range(range_utf16, cx);
        let start = range.start;
        self.edit(range, new_text, window, cx);
        self.marked = (!new_text.is_empty()).then(|| start..start + new_text.len());
        if let Some(selected) = new_selected_range_utf16 {
            let mut utf16 = 0;
            let mut caret = new_text.len();
            for (i, c) in new_text.char_indices() {
                if utf16 >= selected.end {
                    caret = i;
                    break;
                }
                utf16 += c.len_utf16();
            }
            self.caret = start + caret;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.utf16_to_bytes(range_utf16, cx);
        let (bix, lix, index) = self.doc.locate(range.start)?;
        let key = self.doc.blocks[bix].leaves[lix].key;
        let geometry = self.geometry.borrow();
        let leaf = geometry.get(&key)?;
        let start = leaf.position_for_index(index);
        let end = self
            .doc
            .locate(range.end)
            .filter(|(b, l, _)| (*b, *l) == (bix, lix))
            .map_or(start, |(_, _, e)| leaf.position_for_index(e));
        let width = if end.y == start.y {
            (end.x - start.x).max(px(1.))
        } else {
            px(1.)
        };
        Some(Bounds::new(start, size(width, leaf.line_height())))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let src = self.source_for_point(point)?;
        Some(self.editor.read(cx).text().offset_to_offset_utf16(src))
    }
}
