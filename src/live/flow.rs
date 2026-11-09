//! A paragraph or heading as it's drawn: the text a reader sees, its marks
//! (bold, italic, code, links), and for every character where it sits in the
//! Markdown. Edits made on the drawn text become edits of the Markdown at
//! those places, so everything around them stays exactly as written.

use gpui_kit::base::text::editor_host::{LinkMark, TextMark};
use markdown::mdast::Node;
use std::ops::Range;

/// One drawn character and the Markdown it came from.
#[derive(Clone, Debug)]
struct Char {
    /// Its byte range in the drawn text.
    shown: Range<usize>,
    /// Its byte range in the note.
    source: Range<usize>,
}

/// What a marked run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Strong,
    Emphasis,
    Delete,
    Code,
    Link,
    Wiki,
}

impl Wrap {
    /// The delimiter written around a new run of this kind.
    pub fn delimiter(self) -> &'static str {
        match self {
            Wrap::Strong => "**",
            Wrap::Emphasis => "*",
            Wrap::Delete => "~~",
            Wrap::Code => "`",
            Wrap::Link | Wrap::Wiki => "",
        }
    }
}

/// A link under the caret: its Markdown, the text shown and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkAt {
    pub source: Range<usize>,
    pub text: String,
    pub url: String,
    pub wiki: bool,
}

/// A run of marked text (bold, a link…), to drop whole with its delimiters
/// when its last character goes.
#[derive(Clone, Debug)]
struct Wrapper {
    kind: Wrap,
    shown: Range<usize>,
    source: Range<usize>,
    /// Its delimiters as written: `**` and `**`, `[` and `](url)`.
    open: String,
    close: String,
}

#[derive(Clone, Debug, Default)]
pub struct Flow {
    pub heading: Option<u8>,
    pub text: String,
    pub marks: Vec<(Range<usize>, TextMark)>,
    chars: Vec<Char>,
    wrappers: Vec<Wrapper>,
    /// Where text typed into an empty block goes (after a heading's `# `).
    empty_at: usize,
}

/// The drawn form of the block at `range` of `source`, or `None` when it holds
/// something this editor doesn't draw yet (it's then edited as its Markdown).
#[cfg(test)]
pub fn flow(source: &str, range: Range<usize>) -> Option<Flow> {
    let block = &source[range.clone()];
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    let tree = markdown::to_mdast(block, &options).ok()?;
    let [node] = tree.children()?.as_slice() else { return None };
    let (heading, children) = match node {
        Node::Paragraph(p) => (None, &p.children),
        Node::Heading(h) => (Some(h.depth), &h.children),
        _ => return None,
    };
    let mut flow = Flow { heading, empty_at: range.end, ..Default::default() };
    walk(block, range.start, children, TextMark::default(), &mut flow)?;
    Some(flow)
}

/// The drawn form of inline `children` of the note (positions in `source`),
/// such as a list item's text. `empty_at` is where typing goes when empty.
pub fn from_inline(source: &str, heading: Option<u8>, children: &[Node], empty_at: usize) -> Option<Flow> {
    let mut flow = Flow { heading, empty_at, ..Default::default() };
    walk(source, 0, children, TextMark::default(), &mut flow)?;
    Some(flow)
}

fn walk(block: &str, base: usize, nodes: &[Node], mark: TextMark, flow: &mut Flow) -> Option<()> {
    for node in nodes {
        let position = node.position()?;
        let at = position.start.offset..position.end.offset;
        match node {
            Node::Text(text) => push_text(block, base, at, &text.value, &mark, flow)?,
            Node::InlineCode(code) => {
                let ticks = block[at.clone()].chars().take_while(|c| *c == '`').count();
                let inner = at.start + ticks..at.end - ticks;
                let raw = &block[inner.clone()];
                // `` ` x ` `` drops one space each side.
                let inner = if raw.len() >= 2 && raw.starts_with(' ') && raw.ends_with(' ') && raw.trim() != "" {
                    inner.start + 1..inner.end - 1
                } else {
                    inner
                };
                let start = flow.text.len();
                let mut code_mark = mark.clone();
                code_mark.code = true;
                push_text(block, base, inner.clone(), &code.value, &code_mark, flow)?;
                flow.wrappers.push(Wrapper {
                    kind: Wrap::Code,
                    shown: start..flow.text.len(),
                    source: base + at.start..base + at.end,
                    open: block[at.start..inner.start].to_string(),
                    close: block[inner.end..at.end].to_string(),
                });
            }
            Node::Strong(_) | Node::Emphasis(_) | Node::Delete(_) | Node::Link(_) => {
                let children = node.children()?;
                let mut inner_mark = mark.clone();
                match node {
                    Node::Strong(_) => inner_mark.bold = true,
                    Node::Emphasis(_) => inner_mark.italic = true,
                    Node::Delete(_) => inner_mark.strikethrough = true,
                    Node::Link(link) => {
                        inner_mark.link = Some(LinkMark {
                            url: link.url.clone().into(),
                            identifier: None,
                            title: link.title.clone().map(Into::into),
                        })
                    }
                    _ => {}
                }
                let start = flow.text.len();
                walk(block, base, children, inner_mark, flow)?;
                let inner = children.first().and_then(|c| c.position()).map(|p| p.start.offset)?
                    ..children.last().and_then(|c| c.position()).map(|p| p.end.offset)?;
                flow.wrappers.push(Wrapper {
                    kind: match node {
                        Node::Strong(_) => Wrap::Strong,
                        Node::Emphasis(_) => Wrap::Emphasis,
                        Node::Delete(_) => Wrap::Delete,
                        _ => Wrap::Link,
                    },
                    shown: start..flow.text.len(),
                    source: base + at.start..base + at.end,
                    open: block[at.start..inner.start].to_string(),
                    close: block[inner.end..at.end].to_string(),
                });
            }
            Node::Break(_) => {
                let start = flow.text.len();
                flow.text.push('\n');
                flow.chars.push(Char { shown: start..start + 1, source: base + at.start..base + at.end });
            }
            // Images, inline HTML, footnotes, math: not drawn by this editor yet.
            _ => return None,
        }
    }
    Some(())
}

/// Text whose characters come from `at` of the block: the same characters,
/// less the backslashes of escapes.
fn push_text(block: &str, base: usize, at: Range<usize>, value: &str, mark: &TextMark, flow: &mut Flow) -> Option<()> {
    let raw = &block[at.clone()];
    let wikis = wiki_links(value);
    let mut source = raw.char_indices().peekable();
    // Runs of drawn text with one mark: plain, or a wiki link's label.
    let mut run_start = flow.text.len();
    let mut run_link: Option<usize> = None;
    let mut wiki_spans: Vec<(usize, Range<usize>, Range<usize>)> = Vec::new();
    for (vi, c) in value.char_indices() {
        let (mut i, mut s) = source.next()?;
        if s == '\\' && c != '\\' {
            (i, s) = source.next()?;
        }
        if s != c {
            return None; // An entity (`&amp;`) or other rewriting: not drawn yet.
        }
        let char_source = base + at.start + i..base + at.start + i + s.len_utf8();
        // A wiki link shows only its label: its brackets and target are hidden.
        let wiki = wikis.iter().position(|w| w.full.contains(&vi));
        if let Some(w) = wiki {
            if !wikis[w].label.contains(&vi) {
                if vi == wikis[w].full.start {
                    wiki_spans.push((w, flow.text.len()..flow.text.len(), char_source.start..char_source.start));
                }
                if let Some(span) = wiki_spans.iter_mut().find(|(x, ..)| *x == w) {
                    span.2.end = char_source.end;
                }
                continue;
            }
        }
        if run_link != wiki {
            push_mark(flow, run_start, run_link.map(|w| &wikis[w]), mark);
            run_start = flow.text.len();
            run_link = wiki;
        }
        let shown = flow.text.len();
        // A soft line break reads as a space.
        flow.text.push(if c == '\n' { ' ' } else { c });
        flow.chars.push(Char { shown: shown..flow.text.len(), source: char_source.clone() });
        if let Some(span) = wiki.and_then(|w| wiki_spans.iter_mut().find(|(x, ..)| *x == w)) {
            span.1.end = flow.text.len();
            span.2.end = char_source.end;
        }
    }
    push_mark(flow, run_start, run_link.map(|w| &wikis[w]), mark);
    for (w, shown, source) in wiki_spans {
        let wiki = &wikis[w];
        flow.wrappers.push(Wrapper {
            kind: Wrap::Wiki,
            shown,
            source,
            open: value[wiki.full.start..wiki.label.start].to_string(),
            close: value[wiki.label.end..wiki.full.end].to_string(),
        });
    }
    Some(())
}

fn push_mark(flow: &mut Flow, start: usize, wiki: Option<&WikiLink>, mark: &TextMark) {
    if flow.text.len() <= start {
        return;
    }
    let mut mark = mark.clone();
    if let Some(wiki) = wiki {
        mark.link = Some(LinkMark { url: format!("enso-wiki:{}", wiki.target).into(), identifier: None, title: None });
    }
    if mark != TextMark::default() {
        flow.marks.push((start..flow.text.len(), mark));
    }
}

/// `[[target]]`, `[[target|label]]` or `[[target#heading]]` in `text`: the
/// whole link, the part shown (the label, else the target less `#…`), and
/// the target.
struct WikiLink {
    full: Range<usize>,
    label: Range<usize>,
    target: String,
}

fn wiki_links(text: &str) -> Vec<WikiLink> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("[[").map(|i| from + i) {
        let Some(close) = text[open + 2..].find("]]").map(|i| open + 2 + i) else { break };
        let inner = open + 2..close;
        if text[inner.clone()].contains('\n') || inner.is_empty() {
            from = open + 2;
            continue;
        }
        let body = &text[inner.clone()];
        let (target, label) = match body.split_once('|') {
            Some((target, _)) => (target, inner.start + target.len() + 1..inner.end),
            None => {
                let shown = body.split('#').next().unwrap_or(body);
                (body, inner.start..inner.start + shown.len())
            }
        };
        if !label.is_empty() {
            found.push(WikiLink { full: open..close + 2, label, target: target.to_string() });
        }
        from = close + 2;
    }
    found
}

impl Flow {
    /// A new, empty paragraph at `at` of the note.
    pub fn empty(at: usize) -> Self {
        Self { empty_at: at, ..Default::default() }
    }

    /// Text shown exactly as written (a code block's code): every character
    /// is itself in the note, no marks.
    pub fn plain(source: &str, range: Range<usize>) -> Self {
        let mut flow = Self::empty(range.start);
        for (i, c) in source[range.clone()].char_indices() {
            let at = range.start + i;
            let shown = flow.text.len();
            flow.text.push(c);
            flow.chars.push(Char { shown: shown..flow.text.len(), source: at..at + c.len_utf8() });
        }
        flow
    }

    /// Spaces typed after the text, up to the line's end: Markdown drops them
    /// from what it shows, but they're there until more is typed, so they're
    /// drawn and the caret moves past them. Returns where the text now ends.
    pub fn take_trailing_spaces(&mut self, source: &str, end: usize) -> usize {
        // The block's range may already hold the spaces; they start after the
        // last character drawn.
        let last = self.chars.last().map_or(0, |c| c.source.end);
        let mut end = end;
        while end > last && source[..end].ends_with([' ', '\t']) {
            end -= 1;
        }
        let mut at = end;
        for c in source[end..].chars() {
            if c != ' ' && c != '\t' {
                break;
            }
            let shown = self.text.len();
            self.text.push(c);
            self.chars.push(Char { shown: shown..self.text.len(), source: at..at + 1 });
            at += 1;
        }
        // A trailing break (`  ` then a newline) isn't typed spaces.
        if source[at..].starts_with('\n') && at - end >= 2 {
            for _ in end..at {
                self.text.pop();
                self.chars.pop();
            }
            return end;
        }
        at
    }

    /// Where in the note text typed at drawn position `at` goes: after the
    /// character before it (so it takes that character's marks), or before
    /// the first.
    pub fn source_for(&self, at: usize) -> usize {
        match self.chars.iter().rev().find(|c| c.shown.end <= at) {
            Some(c) => c.source.end,
            None => self.chars.first().map_or(self.empty_at, |c| c.source.start),
        }
    }

    /// Where Markdown pasted at drawn position `at` goes: past the
    /// delimiters of any marked run `at` is at the edge of, so it isn't
    /// nested in that run's marks.
    pub fn source_outside_marks(&self, at: usize) -> usize {
        let mut place = self.source_for(at);
        for w in self.wrappers.iter().filter(|w| !w.shown.is_empty()) {
            if w.shown.end == at {
                place = place.max(w.source.end);
            } else if w.shown.start == at && at < self.text.len() {
                place = place.min(w.source.start);
            }
        }
        place
    }

    /// The drawn position of the note's offset `at`.
    pub fn shown_for(&self, at: usize) -> usize {
        self.chars
            .iter()
            .find(|c| c.source.start >= at)
            .map_or(self.text.len(), |c| c.shown.start)
    }

    /// The Markdown to remove for the drawn character just before `at`: the
    /// character, or the whole marked run it was the last of.
    pub fn delete_before(&self, at: usize) -> Option<Range<usize>> {
        let c = self.chars.iter().rev().find(|c| c.shown.end <= at)?;
        Some(self.whole_wrapper(c).unwrap_or(c.source.clone()))
    }

    /// The same, for the character just after `at`.
    pub fn delete_after(&self, at: usize) -> Option<Range<usize>> {
        let c = self.chars.iter().find(|c| c.shown.start >= at)?;
        Some(self.whole_wrapper(c).unwrap_or(c.source.clone()))
    }

    /// The Markdown to remove for drawn text `shown`: each character's bytes,
    /// and any marked run that goes completely, whole (delimiters too), so
    /// nothing is left half-marked. Last first, so each removal leaves the
    /// earlier ones' offsets as they are.
    pub fn delete_shown(&self, shown: Range<usize>) -> Vec<Range<usize>> {
        let gone_whole: Vec<&Wrapper> = self
            .wrappers
            .iter()
            .filter(|w| !w.shown.is_empty() && shown.start <= w.shown.start && w.shown.end <= shown.end)
            .collect();
        let mut ranges: Vec<Range<usize>> = gone_whole.iter().map(|w| w.source.clone()).collect();
        for c in self.chars.iter().filter(|c| shown.start <= c.shown.start && c.shown.end <= shown.end) {
            if !gone_whole.iter().any(|w| w.source.start <= c.source.start && c.source.end <= w.source.end) {
                ranges.push(c.source.clone());
            }
        }
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in ranges {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged.reverse();
        merged
    }

    /// The Markdown of drawn text `shown`, taking in the delimiters of any
    /// marked run it holds whole (`*error messages*`, not `*error messages`).
    pub fn source_span(&self, shown: Range<usize>) -> Range<usize> {
        let inside: Vec<&Char> = self.chars.iter().filter(|c| shown.start <= c.shown.start && c.shown.end <= shown.end).collect();
        let (Some(first), Some(last)) = (inside.first(), inside.last()) else {
            let at = self.source_for(shown.start);
            return at..at;
        };
        let mut span = first.source.start..last.source.end;
        for w in self.wrappers.iter().filter(|w| !w.shown.is_empty() && shown.start <= w.shown.start && w.shown.end <= shown.end) {
            span.start = span.start.min(w.source.start);
            span.end = span.end.max(w.source.end);
        }
        span
    }

    /// The Markdown of drawn text `shown`, its own characters only (inside
    /// any marks around it).
    pub fn char_span(&self, shown: Range<usize>) -> Option<Range<usize>> {
        let first = self.chars.iter().find(|c| c.shown.start >= shown.start && c.shown.end <= shown.end)?;
        let last = self.chars.iter().rev().find(|c| c.shown.start >= shown.start && c.shown.end <= shown.end)?;
        Some(first.source.start..last.source.end)
    }

    /// The edits (last first) that toggle `kind` on drawn text `shown`: off
    /// when it's all inside one run of that kind (the run's delimiters go),
    /// else on (delimiters around it, spaces at its ends left outside).
    pub fn toggle(&self, kind: Wrap, shown: Range<usize>) -> Vec<(Range<usize>, String)> {
        if let Some(w) = self
            .wrappers
            .iter()
            .filter(|w| w.kind == kind && w.shown.start <= shown.start && shown.end <= w.shown.end)
            .min_by_key(|w| w.source.len())
        {
            let close = w.source.end - w.close.len()..w.source.end;
            let open = w.source.start..w.source.start + w.open.len();
            return vec![(close, String::new()), (open, String::new())];
        }
        let text = &self.text[shown.clone()];
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        if lead + trail >= text.len() {
            return Vec::new();
        }
        let Some(span) = self.char_span(shown.start + lead..shown.end - trail) else { return Vec::new() };
        let mark = kind.delimiter().to_string();
        vec![(span.end..span.end, mark.clone()), (span.start..span.start, mark)]
    }

    /// The link whose letter is at drawn position `at` (under the pointer).
    pub fn link_under(&self, at: usize) -> Option<LinkAt> {
        let inside = self.wrappers.iter().any(|w| matches!(w.kind, Wrap::Link | Wrap::Wiki) && w.shown.start <= at && at < w.shown.end);
        if inside { self.link_at(at) } else { None }
    }

    /// The link at drawn position `at` (its text, inclusive of both ends).
    pub fn link_at(&self, at: usize) -> Option<LinkAt> {
        let w = self
            .wrappers
            .iter()
            .filter(|w| matches!(w.kind, Wrap::Link | Wrap::Wiki) && w.shown.start <= at && at <= w.shown.end && !w.shown.is_empty())
            .min_by_key(|w| w.source.len())?;
        let url = self
            .marks
            .iter()
            .find(|(r, m)| r.start < w.shown.end && w.shown.start < r.end && m.link.is_some())
            .and_then(|(_, m)| m.link.as_ref())
            .map(|l| l.url.to_string())?;
        Some(LinkAt { source: w.source.clone(), text: self.text[w.shown.clone()].to_string(), url, wiki: w.kind == Wrap::Wiki })
    }

    /// Where the word before `at` starts (spaces before it skipped).
    pub fn word_start(&self, at: usize) -> usize {
        let before: Vec<(usize, char)> = self.text[..at].char_indices().collect();
        let mut i = before.len();
        while i > 0 && before[i - 1].1.is_whitespace() {
            i -= 1;
        }
        let word = i > 0 && is_word(before[i - 1].1);
        while i > 0 && !before[i - 1].1.is_whitespace() && is_word(before[i - 1].1) == word {
            i -= 1;
        }
        before.get(i).map_or(at, |(b, _)| *b).min(at)
    }

    /// Where the word after `at` ends (spaces before it skipped).
    pub fn word_end(&self, at: usize) -> usize {
        let mut chars = self.text[at..].char_indices().peekable();
        let mut end = at;
        while let Some((i, c)) = chars.peek().copied() {
            if !c.is_whitespace() {
                break;
            }
            end = at + i + c.len_utf8();
            chars.next();
        }
        let word = chars.peek().is_some_and(|(_, c)| is_word(*c));
        while let Some((i, c)) = chars.next() {
            if c.is_whitespace() || is_word(c) != word {
                break;
            }
            end = at + i + c.len_utf8();
        }
        end
    }

    fn whole_wrapper(&self, c: &Char) -> Option<Range<usize>> {
        self.wrappers
            .iter()
            .filter(|w| w.shown == c.shown)
            .map(|w| w.source.clone())
            .max_by_key(|r| r.len())
    }

    /// The Markdown that splits the block at drawn position `at` into two
    /// paragraphs: where it goes, and the text (closing and reopening any
    /// marks around it).
    pub fn split_at(&self, at: usize) -> (usize, String) {
        let mut close = String::new();
        let mut open = String::new();
        let mut place = self.source_for(at);
        // Innermost first when closing; outermost first when reopening.
        let mut around: Vec<&Wrapper> = self
            .wrappers
            .iter()
            .filter(|w| w.shown.start < at && at < w.shown.end)
            .collect();
        around.sort_by_key(|w| w.source.len());
        for w in &around {
            close.push_str(&w.close);
        }
        for w in around.iter().rev() {
            open.push_str(&w.open);
        }
        // At the very end of a marked run, split after its closing delimiter.
        if let Some(w) = self.wrappers.iter().filter(|w| w.shown.end == at && w.shown.start < at).max_by_key(|w| w.source.len()) {
            place = w.source.end;
        }
        (place, format!("{close}\n\n{open}"))
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(source: &str) -> Flow {
        flow(source, 0..source.trim_end().len()).unwrap()
    }

    #[test]
    fn drawn_text_and_where_each_letter_lives() {
        let source = "Plain **bold** and `code` \\* [link](https://a.io)";
        let f = of(source);
        assert_eq!(f.text, "Plain bold and code * link");
        // Typing after "bol" goes inside the bold.
        let at = f.text.find("bold").unwrap() + 3;
        assert_eq!(&source[..f.source_for(at)], "Plain **bol");
        assert_eq!(f.shown_for(f.source_for(at)), at);
        // Deleting a letter removes just it; the last letter of a run takes
        // its delimiters along.
        assert_eq!(&source[f.delete_before(at).unwrap()], "l");
        let single = of("a **b** c");
        assert_eq!(&"a **b** c"[single.delete_before(3).unwrap()], "**b**");
    }

    #[test]
    fn words_and_ranges_delete_cleanly() {
        let source = "Pack **two bags** now";
        let f = of(source);
        assert_eq!(f.text, "Pack two bags now");
        assert_eq!(f.word_start(f.text.len()), f.text.find("now").unwrap());
        assert_eq!(f.word_start(f.text.find(" now").unwrap()), f.text.find("bags").unwrap());
        assert_eq!(f.word_end(0), 4);
        // Deleting "two bags" removes the bold with it, leaving no `****`.
        let mut out = source.to_string();
        for r in f.delete_shown(5..13) {
            out.replace_range(r, "");
        }
        assert_eq!(out, "Pack  now");
        // Deleting "o b" keeps the bold around what's left.
        let mut out = source.to_string();
        for r in f.delete_shown(7..10) {
            out.replace_range(r, "");
        }
        assert_eq!(out, "Pack **twags** now");
    }

    #[test]
    fn copied_markdown_takes_whole_marks() {
        let source = "Review **empty states** and *error messages*";
        let f = of(source);
        let all = f.source_span(0..f.text.len());
        assert_eq!(&source[all], source);
        let words = f.text.find("empty").unwrap()..f.text.find(" and").unwrap();
        assert_eq!(&source[f.source_span(words)], "**empty states**");
    }

    #[test]
    fn marks_toggle_on_and_off() {
        let apply = |source: &str, edits: Vec<(Range<usize>, String)>| {
            let mut out = source.to_string();
            for (r, t) in edits {
                out.replace_range(r, &t);
            }
            out
        };
        let source = "Make this bold now";
        let f = of(source);
        let this = f.text.find("this bold").unwrap()..f.text.find(" now").unwrap();
        assert_eq!(apply(source, f.toggle(Wrap::Strong, this)), "Make **this bold** now");
        let bold = "Make **this bold** now";
        let f = of(bold);
        let inside = f.text.find("bold").unwrap()..f.text.find(" now").unwrap();
        assert_eq!(apply(bold, f.toggle(Wrap::Strong, inside)), "Make this bold now");
        // Spaces at the selection's ends stay outside the marks.
        let f = of(source);
        assert_eq!(apply(source, f.toggle(Wrap::Emphasis, 4..10)), "Make *this* bold now");
        let linked = "See [the docs](https://a.io) here";
        let f = of(linked);
        let at = f.text.find("docs").unwrap();
        let link = f.link_at(at).unwrap();
        assert_eq!((link.text.as_str(), link.url.as_str(), &linked[link.source]), ("the docs", "https://a.io", "[the docs](https://a.io)"));
    }

    #[test]
    fn wiki_links_show_their_label() {
        let source = "See [[19-retrospective]] and [[plan|the plan]].";
        let f = of(source);
        assert_eq!(f.text, "See 19-retrospective and the plan.");
        // Typing in the label edits the label.
        let at = f.text.find("plan").unwrap();
        assert_eq!(&source[..f.source_for(at)], "See [[19-retrospective]] and [[plan|the ");
        // The whole link goes with its last letter.
        let one = of("a [[b]] c");
        assert_eq!(&"a [[b]] c"[one.delete_before(3).unwrap()], "[[b]]");
    }

    #[test]
    fn headings_and_splits() {
        let source = "## Big **title** here";
        let f = of(source);
        assert_eq!((f.heading, f.text.as_str()), (Some(2), "Big title here"));
        // Inside the bold: close it, start the paragraph, reopen it.
        let (at, text) = f.split_at(6);
        assert_eq!((&source[..at], text.as_str()), ("## Big **ti", "**\n\n**"));
        // At its end: after the delimiter, nothing to reopen.
        let (at, text) = f.split_at(9);
        assert_eq!((&source[..at], text.as_str()), ("## Big **title**", "\n\n"));
        // Unsupported inline content: no flow.
        assert!(flow("an ![img](a.png) here", 0..21).is_none());
    }
}
