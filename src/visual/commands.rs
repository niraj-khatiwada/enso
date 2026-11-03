use std::ops::Range;

use gpui_kit::base::input::RopeExt as _;
use gpui_kit::{
    App, ClipboardItem, Context, Div, InteractiveElement as _, KeyBinding, Stateful, Window,
    actions,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::markdown_edit::{self, Block, Inline};

use super::prefs::{FocusMode, WritingPrefs};
use super::view::{CONTEXT, VisualView};

actions!(
    visual_editor,
    [
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectLineStart,
        SelectLineEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Bold,
        Italic,
        Strikethrough,
        InlineCode,
        InsertLink,
        Heading1,
        Heading2,
        Heading3,
        Heading4,
        Heading5,
        Heading6,
        BodyText,
        BulletList,
        NumberedList,
        TaskList,
        ToggleTaskDone,
        Quote,
        CodeBlock,
        IndentListItem,
        OutdentListItem,
        ToggleTypewriter,
        CycleFocusMode,
        TableRowAbove,
        TableRowBelow,
        TableColumnLeft,
        TableColumnRight,
        TableDeleteRow,
        TableDeleteColumn,
        TableTidy,
        FollowLink,
        LargerText,
        SmallerText
    ]
);

pub fn init(cx: &mut App) {
    macro_rules! key {
        ($keys:literal) => {
            if cfg!(target_os = "macos") {
                concat!("cmd-", $keys)
            } else {
                concat!("ctrl-", $keys)
            }
        };
    }
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new(key!("a"), SelectAll, c),
        KeyBinding::new(key!("c"), Copy, c),
        KeyBinding::new(key!("x"), Cut, c),
        KeyBinding::new(key!("v"), Paste, c),
        KeyBinding::new(key!("b"), Bold, c),
        KeyBinding::new(key!("i"), Italic, c),
        KeyBinding::new(key!("shift-x"), Strikethrough, c),
        KeyBinding::new(key!("shift-c"), InlineCode, c),
        KeyBinding::new(key!("k"), InsertLink, c),
        KeyBinding::new(key!("1"), Heading1, c),
        KeyBinding::new(key!("2"), Heading2, c),
        KeyBinding::new(key!("3"), Heading3, c),
        KeyBinding::new(key!("4"), Heading4, c),
        KeyBinding::new(key!("5"), Heading5, c),
        KeyBinding::new(key!("6"), Heading6, c),
        KeyBinding::new(key!("0"), BodyText, c),
        KeyBinding::new(key!("shift-8"), BulletList, c),
        KeyBinding::new(key!("shift-7"), NumberedList, c),
        KeyBinding::new(key!("shift-l"), TaskList, c),
        KeyBinding::new(key!("enter"), ToggleTaskDone, c),
        KeyBinding::new(key!("shift-."), Quote, c),
        KeyBinding::new(key!("alt-c"), CodeBlock, c),
        KeyBinding::new("tab", IndentListItem, c),
        KeyBinding::new("shift-tab", OutdentListItem, c),
        KeyBinding::new(key!("shift-t"), ToggleTypewriter, c),
        KeyBinding::new(key!("shift-d"), CycleFocusMode, c),
        KeyBinding::new(key!("alt-up"), TableRowAbove, c),
        KeyBinding::new(key!("alt-down"), TableRowBelow, c),
        KeyBinding::new(key!("alt-left"), TableColumnLeft, c),
        KeyBinding::new(key!("alt-right"), TableColumnRight, c),
        KeyBinding::new(key!("alt-backspace"), TableDeleteRow, c),
        KeyBinding::new(key!("alt-shift-backspace"), TableDeleteColumn, c),
        KeyBinding::new(key!("alt-t"), TableTidy, c),
        KeyBinding::new("alt-enter", FollowLink, c),
        KeyBinding::new(key!("="), LargerText, c),
        KeyBinding::new(key!("+"), LargerText, c),
        KeyBinding::new(key!("-"), SmallerText, c),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-left", super::view::MoveLineStart, c),
        KeyBinding::new("cmd-right", super::view::MoveLineEnd, c),
        KeyBinding::new("cmd-shift-left", SelectLineStart, c),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
    ]);
}

pub fn listeners(root: Stateful<Div>, cx: &mut Context<VisualView>) -> Stateful<Div> {
    macro_rules! on {
        ($root:expr, $action:ty, |$this:ident, $window:ident, $cx:ident| $body:expr) => {
            $root.on_action(cx.listener(|$this, _: &$action, $window, $cx| {
                $this.perf.borrow_mut().input();
                $body;
                $this.perf.borrow_mut().handler_done();
            }))
        };
    }
    let root = on!(root, SelectLeft, |this, _w, cx| this
        .move_horizontal(false, true, cx));
    let root = on!(root, SelectRight, |this, _w, cx| this
        .move_horizontal(true, true, cx));
    let root = on!(root, SelectUp, |this, _w, cx| this
        .move_vertical(false, true, cx));
    let root = on!(root, SelectDown, |this, _w, cx| this
        .move_vertical(true, true, cx));
    let root = on!(root, SelectLineStart, |this, _w, cx| this
        .line_edge(false, true, cx));
    let root = on!(root, SelectLineEnd, |this, _w, cx| this
        .line_edge(true, true, cx));
    let root = on!(root, SelectAll, |this, _w, cx| this.select_all(cx));
    let root = on!(root, Copy, |this, _w, cx| this.copy(cx));
    let root = on!(root, Cut, |this, w, cx| this.cut(w, cx));
    let root = on!(root, Paste, |this, w, cx| this.paste(w, cx));
    fn inline(inline: Inline) -> impl Fn(&str, Range<usize>) -> Option<markdown_edit::Edit> {
        move |text, sel| Some(markdown_edit::toggle_inline(text, sel, inline))
    }
    fn block(block: Block) -> impl Fn(&str, Range<usize>) -> Option<markdown_edit::Edit> {
        move |text, sel| Some(markdown_edit::toggle_block(text, sel, block))
    }
    let root = on!(root, Bold, |this, w, cx| this.apply(
        inline(Inline::Bold),
        w,
        cx
    ));
    let root = on!(root, Italic, |this, w, cx| this.apply(
        inline(Inline::Italic),
        w,
        cx
    ));
    let root = on!(root, Strikethrough, |this, w, cx| this.apply(
        inline(Inline::Strikethrough),
        w,
        cx
    ));
    let root = on!(root, InlineCode, |this, w, cx| this.apply(
        inline(Inline::Code),
        w,
        cx
    ));
    let root = on!(root, InsertLink, |this, w, cx| this.apply(
        |t: &str, s| Some(markdown_edit::link(t, s)),
        w,
        cx
    ));
    let root = on!(root, Heading1, |this, w, cx| this.apply(
        block(Block::Heading(1)),
        w,
        cx
    ));
    let root = on!(root, Heading2, |this, w, cx| this.apply(
        block(Block::Heading(2)),
        w,
        cx
    ));
    let root = on!(root, Heading3, |this, w, cx| this.apply(
        block(Block::Heading(3)),
        w,
        cx
    ));
    let root = on!(root, Heading4, |this, w, cx| this.apply(
        block(Block::Heading(4)),
        w,
        cx
    ));
    let root = on!(root, Heading5, |this, w, cx| this.apply(
        block(Block::Heading(5)),
        w,
        cx
    ));
    let root = on!(root, Heading6, |this, w, cx| this.apply(
        block(Block::Heading(6)),
        w,
        cx
    ));
    let root = on!(root, BodyText, |this, w, cx| this.apply(
        block(Block::Heading(0)),
        w,
        cx
    ));
    let root = on!(root, BulletList, |this, w, cx| this.apply(
        block(Block::Bullet),
        w,
        cx
    ));
    let root = on!(root, NumberedList, |this, w, cx| this.apply(
        block(Block::Numbered),
        w,
        cx
    ));
    let root = on!(root, TaskList, |this, w, cx| this.apply(
        block(Block::Task),
        w,
        cx
    ));
    let root = on!(root, Quote, |this, w, cx| this.apply(
        block(Block::Quote),
        w,
        cx
    ));
    let root = on!(
        root,
        ToggleTaskDone,
        |this, w, cx| if this.math_dialog.is_some() {
            this.confirm_math_dialog(w, cx);
        } else {
            this.apply(
                |t: &str, s| Some(markdown_edit::toggle_task_done(t, s)),
                w,
                cx,
            );
        }
    );
    let root = on!(root, CodeBlock, |this, w, cx| this.apply(
        |t: &str, s| Some(markdown_edit::toggle_code_block(t, s)),
        w,
        cx
    ));
    let root = on!(root, IndentListItem, |this, w, cx| this
        .indent(false, w, cx));
    let root = on!(root, OutdentListItem, |this, w, cx| this
        .indent(true, w, cx));
    use super::table_ui::TableOp;
    let root = on!(root, TableRowAbove, |this, w, cx| this.table_op(
        TableOp::RowAbove,
        w,
        cx
    ));
    let root = on!(root, TableRowBelow, |this, w, cx| this.table_op(
        TableOp::RowBelow,
        w,
        cx
    ));
    let root = on!(root, TableColumnLeft, |this, w, cx| this.table_op(
        TableOp::ColumnLeft,
        w,
        cx
    ));
    let root = on!(root, TableColumnRight, |this, w, cx| this.table_op(
        TableOp::ColumnRight,
        w,
        cx
    ));
    let root = on!(root, TableDeleteRow, |this, w, cx| this.table_op(
        TableOp::DeleteRow,
        w,
        cx
    ));
    let root = on!(root, TableDeleteColumn, |this, w, cx| this.table_op(
        TableOp::DeleteColumn,
        w,
        cx
    ));
    let root = on!(root, FollowLink, |this, _w, cx| this.follow_at_caret(cx));
    let root = on!(root, LargerText, |this, _w, cx| step_text_size(1., cx));
    let root = on!(root, SmallerText, |this, _w, cx| step_text_size(-1., cx));
    let root = on!(root, TableTidy, |this, w, cx| this.table_op(
        TableOp::Tidy,
        w,
        cx
    ));
    let root = on!(root, ToggleTypewriter, |this, _w, cx| {
        let mut prefs = WritingPrefs::get(cx);
        prefs.typewriter = !prefs.typewriter;
        cx.set_global(prefs);
    });
    on!(root, CycleFocusMode, |this, _w, cx| {
        let mut prefs = WritingPrefs::get(cx);
        prefs.focus = match prefs.focus {
            FocusMode::Off => FocusMode::Sentence,
            FocusMode::Sentence => FocusMode::Paragraph,
            FocusMode::Paragraph => FocusMode::Off,
        };
        cx.set_global(prefs);
    })
}

impl VisualView {
    pub fn apply(
        &mut self,
        command: impl FnOnce(&str, Range<usize>) -> Option<markdown_edit::Edit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let selection = self.selection();
        let (start, text) = {
            let rope = self.editor.read(cx).text();
            let first = rope.offset_to_point(selection.start).row.saturating_sub(1);
            let last = rope.offset_to_point(selection.end).row + 1;
            let start = rope.line_start_offset(first);
            let end = rope.line_end_offset(last).max(selection.end);
            (start, rope.slice(start..end).to_string())
        };
        let Some(edit) = command(&text, selection.start - start..selection.end - start) else {
            return false;
        };
        self.edit_and_select(
            edit.range.start + start..edit.range.end + start,
            &edit.insert,
            edit.select.start + start..edit.select.end + start,
            window,
            cx,
        );
        true
    }

    pub fn enter_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selection().is_empty() {
            let selection = self.selection();
            self.edit(selection, "", window, cx);
        }
        let caret = self.caret;
        if self.apply(
            |text, sel| markdown_edit::continue_on_enter(text, sel.start),
            window,
            cx,
        ) {
            return;
        }
        let in_code = self.doc.locate(caret).is_some_and(|(b, _, _)| {
            matches!(
                self.doc.blocks[b].node,
                gpui_kit::base::text::editor_host::BlockNode::CodeBlock(_)
            )
        });
        self.edit(
            caret..caret,
            if in_code { "\n" } else { "\n\n" },
            window,
            cx,
        );
    }

    fn indent(&mut self, outdent: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.next_cell(outdent, window, cx) {
            return;
        }
        self.apply(
            |text, sel| markdown_edit::indent_list(text, sel, outdent),
            window,
            cx,
        );
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let len = self.editor.read(cx).text().len();
        self.anchor = Some(0);
        self.caret = len;
        cx.notify();
    }

    pub fn copy(&mut self, cx: &mut Context<Self>) {
        let selection = self.selection();
        if selection.is_empty() {
            return;
        }
        let text = self.editor.read(cx).text().slice(selection).to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub fn cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selection();
        if selection.is_empty() {
            return;
        }
        self.copy(cx);
        self.edit(selection, "", window, cx);
    }

    pub fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if let Some(hook) = self.paste_images.clone()
            && let Some(links) = hook(&item, cx)
        {
            let mut at = self.selection().end;
            for (url, alt) in links {
                let image = super::image::ImageBlock {
                    url,
                    alt,
                    ..Default::default()
                };
                self.write_image(super::image_ui::ImageTarget::Insert(at), &image, window, cx);
                if let Some((ix, _)) = self.selected_image() {
                    at = self.doc.blocks[ix].end;
                }
            }
            return;
        }
        let Some(text) = item.text() else {
            return;
        };
        let crlf = self.editor.read(cx).text().to_string().contains("\r\n");
        let text = text.replace("\r\n", "\n");
        let text = if crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        };
        let selection = self.selection();
        self.edit(selection, &text, window, cx);
    }

    pub fn select_word(&mut self, cx: &mut Context<Self>) {
        let Some((bix, lix, index)) = self.doc.locate(self.caret) else {
            return;
        };
        let leaf = &self.doc.blocks[bix].leaves[lix];
        let word = leaf
            .text
            .split_word_bound_indices()
            .map(|(i, w)| i..i + w.len())
            .find(|r| r.start <= index && index < r.end.max(r.start + 1))
            .or_else(|| {
                leaf.text
                    .split_word_bound_indices()
                    .map(|(i, w)| i..i + w.len())
                    .next_back()
            });
        if let Some(word) = word {
            self.anchor = Some(self.doc.src_of(bix, lix, word.start));
            self.caret = self.doc.src_of(bix, lix, word.end);
            cx.notify();
        }
    }

    pub fn select_paragraph(&mut self, cx: &mut Context<Self>) {
        let Some((bix, lix, _)) = self.doc.locate(self.caret) else {
            return;
        };
        let len = self.doc.blocks[bix].leaves[lix].len();
        self.anchor = Some(self.doc.src_of(bix, lix, 0));
        self.caret = self.doc.src_of(bix, lix, len);
        cx.notify();
    }
}

fn step_text_size(step: f32, cx: &mut App) {
    let mut prefs = WritingPrefs::get(cx);
    let size = (prefs.text_size + step).clamp(super::prefs::MIN_TEXT, super::prefs::MAX_TEXT);
    if size != prefs.text_size {
        prefs.text_size = size;
        cx.set_global(prefs);
    }
}

pub fn footnote_at(text: &str, offset: usize) -> Option<(String, bool)> {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = &text[line_start..line_end];
    let at = offset - line_start;
    let mut search = 0;
    while let Some(open) = line[search..].find("[^").map(|i| search + i) {
        let Some(close) = line[open..].find(']').map(|i| open + i) else {
            break;
        };
        if open <= at && at <= close + 1 {
            let id = line[open + 2..close].to_string();
            let definition = open == 0 && line[close + 1..].starts_with(':');
            return (!id.is_empty()).then_some((id, definition));
        }
        search = close + 1;
    }
    None
}

pub fn link_at(text: &str, offset: usize) -> Option<(String, bool)> {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = &text[line_start..line_end];
    let at = offset - line_start;
    let mut search = 0;
    while let Some(open) = line[search..].find('[').map(|i| search + i) {
        let Some(close) = line[open..].find(']').map(|i| open + i) else {
            break;
        };
        let rest = &line[close + 1..];
        let (end, target, reference) = if let Some(inner) = rest.strip_prefix('(') {
            let Some(len) = inner.find(')') else {
                search = close + 1;
                continue;
            };
            let url = inner[..len].split_whitespace().next().unwrap_or("");
            (
                close + 2 + len,
                url.trim_matches(['<', '>']).to_string(),
                false,
            )
        } else if let Some(inner) = rest.strip_prefix('[') {
            let Some(len) = inner.find(']') else {
                search = close + 1;
                continue;
            };
            let id = if len == 0 {
                line[open + 1..close].to_string()
            } else {
                inner[..len].to_string()
            };
            (close + 2 + len, id, true)
        } else {
            (close + 1, line[open + 1..close].to_string(), true)
        };
        if open <= at && at <= end && !line[open..].starts_with("[^") {
            return Some((target, reference));
        }
        search = close + 1;
    }
    None
}

impl VisualView {
    pub fn follow_footnote(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
        let text = self.editor.read(cx).text().to_string();
        let Some((id, definition)) = footnote_at(&text, offset) else {
            return false;
        };
        let label = format!("[^{id}]");
        let target = if definition {
            text.match_indices(&label)
                .map(|(i, _)| i)
                .find(|&i| !text[i + label.len()..].starts_with(':'))
        } else {
            text.match_indices(&format!("{label}:"))
                .map(|(i, _)| i)
                .find(|&i| i == 0 || text[..i].ends_with('\n'))
                .map(|i| {
                    let after = i + label.len() + 1;
                    after + text[after..].len() - text[after..].trim_start_matches(' ').len()
                })
        };
        let Some(target) = target else {
            return false;
        };
        self.place_caret(target, None, cx);
        true
    }

    pub fn follow_at_caret(&mut self, cx: &mut Context<Self>) {
        let caret = self.caret;
        if self.follow_footnote(caret, cx) {
            return;
        }
        let text = self.editor.read(cx).text().to_string();
        let Some((target, reference)) = link_at(&text, caret) else {
            return;
        };
        let url = if reference {
            let id = target.to_lowercase();
            self.doc
                .link_references()
                .into_iter()
                .find(|(identifier, _, _)| identifier.to_lowercase() == id)
                .map(|(_, url, _)| url)
        } else {
            Some(target)
        };
        let Some(url) = url else {
            return;
        };
        if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:") {
            cx.open_url(&url);
        } else if !url.contains(':')
            && !url.starts_with('#')
            && let Some(base) = &self.base
        {
            cx.reveal_path(&base.join(&url));
        }
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;

    #[test]
    fn finds_footnotes_and_links_at_an_offset() {
        let text = "See this[^note] and [docs](https://a.example \"T\") or [ref][r].\n\n[^note]: The note.\n";
        assert_eq!(footnote_at(text, 10), Some(("note".into(), false)));
        let def = text.find("[^note]:").unwrap();
        assert_eq!(footnote_at(text, def + 2), Some(("note".into(), true)));
        assert_eq!(footnote_at(text, 2), None);
        let docs = text.find("docs").unwrap();
        assert_eq!(
            link_at(text, docs),
            Some(("https://a.example".into(), false))
        );
        let r = text.find("ref").unwrap();
        assert_eq!(link_at(text, r), Some(("r".into(), true)));
        assert_eq!(link_at(text, 10), None);
    }
}
