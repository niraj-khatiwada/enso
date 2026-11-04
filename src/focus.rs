//! Focus Mode: the page alone. The sidebar, tabs, outline and status bar
//! step away, the text sits in a centred column, and while writing the line
//! with the cursor stays in the middle of the view with every paragraph but
//! the current one faded.

use super::*;
use gpui_kit::component::input::{TextDecoration, TextDecorationCollection};

/// How far the paragraphs around the current one fade.
const FADE: f32 = 0.65;
/// The text column's width in Focus Mode.
pub(crate) const COLUMN_WIDTH: f32 = 760.;

/// What Focus Mode last did in the editor being written in.
pub(crate) struct FocusTrack {
    editor: EntityId,
    pub(crate) decorations: TextDecorationCollection,
    /// The cursor the view was last centred on.
    centred: Option<usize>,
    /// A cursor seen moving, centred once a frame has laid it out.
    waiting: Option<usize>,
}

impl Enso {
    /// From Split, Focus Mode shows the side you came to Split from (Read or
    /// Edit), and Split returns when it ends.
    pub(crate) fn toggle_focus_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_mode {
            self.split_before_focus = self.mode == Mode::Split;
            if self.split_before_focus {
                let single = if self.previous_mode == Mode::Split { Mode::Edit } else { self.previous_mode };
                self.set_mode(single, window, cx);
            }
            self.focus_mode = true;
        } else {
            self.focus_mode = false;
            self.end_focus_track(cx);
            if std::mem::take(&mut self.split_before_focus) {
                self.set_mode(Mode::Split, window, cx);
            }
        }
        cx.notify();
    }

    fn end_focus_track(&mut self, cx: &mut Context<Self>) {
        if let Some(track) = self.focus_track.take() {
            track.decorations.clear(cx);
        }
    }

    /// Each frame in Focus Mode: once the cursor has moved and been laid out,
    /// centre its line and fade all but its paragraph.
    pub(crate) fn focus_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_mode {
            return;
        }
        let editor = self
            .active
            .map(|i| &self.documents[i])
            .filter(|doc| self.mode != Mode::Read && doc.editor_loaded)
            .map(|doc| doc.editor.clone());
        let Some(editor) = editor else {
            self.end_focus_track(cx);
            return;
        };
        if self.focus_track.as_ref().is_some_and(|t| t.editor != editor.entity_id()) {
            self.end_focus_track(cx);
        }
        if self.focus_track.is_none() {
            let decorations = editor.update(cx, |e, cx| e.create_decorations_collection(Vec::new(), cx));
            self.focus_track = Some(FocusTrack {
                editor: editor.entity_id(),
                decorations,
                centred: None,
                waiting: None,
            });
        }
        let cursor = editor.read(cx).cursor();
        let track = self.focus_track.as_mut().unwrap();
        if track.centred == Some(cursor) {
            return;
        }
        // The caret's place is known after the frame that moves it.
        if track.waiting != Some(cursor) {
            track.waiting = Some(cursor);
            window.request_animation_frame();
            return;
        }
        track.waiting = None;
        track.centred = Some(cursor);
        let text = editor.read(cx).value();
        let paragraph = paragraph_at(&text, cursor);
        let faded = HighlightStyle { fade_out: Some(FADE), ..Default::default() };
        let decorations = [0..paragraph.start, paragraph.end..text.len()]
            .into_iter()
            .filter(|r| !r.is_empty())
            .map(|r| TextDecoration::new(r, faded))
            .collect();
        track.decorations.set(decorations, cx);
        // A click puts the cursor where it lands; only keyboard moves centre.
        if self.clicked {
            return;
        }
        if let Some(caret) = editor.read(cx).prepaint_caret_geometry() {
            let delta = caret.cursor_bounds().center().y - caret.input_bounds().center().y;
            if delta.abs() > px(2.) {
                let mut offset = caret.scroll_offset();
                offset.y -= delta;
                editor.update(cx, |e, cx| e.set_scroll_offset(offset, cx));
            }
        }
    }
}

/// The paragraph (text between blank lines) holding `offset`.
pub(crate) fn paragraph_at(text: &str, offset: usize) -> std::ops::Range<usize> {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind("\n\n").map_or(0, |i| i + 2);
    let end = text[offset..].find("\n\n").map_or(text.len(), |i| offset + i);
    start..end.max(start)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_paragraph_runs_between_blank_lines() {
        let text = "# Title\n\nFirst line\nsame paragraph.\n\nNext one.";
        let at = text.find("same").unwrap();
        assert_eq!(&text[super::paragraph_at(text, at)], "First line\nsame paragraph.");
        assert_eq!(&text[super::paragraph_at(text, 0)], "# Title");
        assert_eq!(&text[super::paragraph_at(text, text.len())], "Next one.");
    }
}
