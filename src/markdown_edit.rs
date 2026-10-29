//! Markdown formatting commands for the editor, as pure text transforms.
//!
//! Each command takes some text and a selection (UTF-8 byte offsets) and
//! returns one [`Edit`]: a range to replace, its replacement, and where the
//! selection lands afterwards, so every command is a single undo step. The
//! caller passes only the lines around the selection, not the whole document.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub insert: String,
    pub select: Range<usize>,
}

/// Inline markers a selection can be wrapped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inline {
    Bold,
    Italic,
    Strikethrough,
    Code,
}

impl Inline {
    fn marker(self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Strikethrough => "~~",
            Self::Code => "`",
        }
    }
}

/// Line-level formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// `0` turns a heading back into body text.
    Heading(usize),
    Bullet,
    Numbered,
    Task,
    Quote,
}

/// How many `ch` characters run back from `end` / forward from `start`.
fn run_before(text: &str, end: usize, ch: char) -> usize {
    text[..end].chars().rev().take_while(|&c| c == ch).count()
}
fn run_after(text: &str, start: usize, ch: char) -> usize {
    text[start..].chars().take_while(|&c| c == ch).count()
}

/// Whether `inline` already surrounds `range` (its markers sit just outside).
/// Stars are counted as runs, so italic and bold inside `***x***` are told
/// apart, and italic is not mistaken for half of a bold marker.
fn surrounds(text: &str, range: &Range<usize>, inline: Inline) -> bool {
    if matches!(inline, Inline::Bold | Inline::Italic) {
        let run = run_before(text, range.start, '*').min(run_after(text, range.end, '*'));
        return match inline {
            Inline::Bold => run == 2 || run == 3,
            _ => run == 1 || run == 3,
        };
    }
    let marker = inline.marker();
    text[..range.start].ends_with(marker) && text[range.end..].starts_with(marker)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// Bold, italic, strikethrough or inline code: wrap the selection (or the
/// word under the cursor, or an empty pair to type into), or unwrap it when
/// it is already formatted that way.
pub fn toggle_inline(text: &str, selection: Range<usize>, inline: Inline) -> Edit {
    let marker = inline.marker();
    let m = marker.len();
    let mut range = selection.clone();

    if range.is_empty() {
        // Inside an empty pair, like `**|**`: take it back out.
        if text[..range.start].ends_with(marker) && text[range.start..].starts_with(marker) {
            let at = range.start - m;
            return Edit {
                range: at..range.start + m,
                insert: String::new(),
                select: at..at,
            };
        }
        // Otherwise act on the word the cursor is in, if any.
        let start = text[..range.start]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word_char(*c))
            .last()
            .map_or(range.start, |(i, _)| i);
        let end = range.start
            + text[range.start..]
                .char_indices()
                .take_while(|(_, c)| is_word_char(*c))
                .last()
                .map_or(0, |(i, c)| i + c.len_utf8());
        if start == end {
            let at = range.start;
            return Edit {
                range: at..at,
                insert: format!("{marker}{marker}"),
                select: at + m..at + m,
            };
        }
        range = start..end;
    } else {
        // Whitespace at the edges stays outside the markers: `** a**` is not bold.
        let selected = &text[range.clone()];
        range.start += selected.len() - selected.trim_start().len();
        range.end -= selected.len() - selected.trim_end().len();
        if range.start >= range.end {
            return Edit {
                range: selection.clone(),
                insert: text[selection.clone()].to_string(),
                select: selection,
            };
        }
    }
    let cursor_offset = selection
        .is_empty()
        .then(|| selection.start - range.start);
    let inner = &text[range.clone()];

    // Selected with its markers, like `**bold**`.
    if inner.len() >= 2 * m
        && inner.starts_with(marker)
        && inner.ends_with(marker)
        && !surrounds(text, &range, inline)
        && (inline != Inline::Italic || !inner.starts_with("**") || inner.starts_with("***"))
    {
        let unwrapped = &inner[m..inner.len() - m];
        return Edit {
            range: range.clone(),
            insert: unwrapped.to_string(),
            select: range.start..range.start + unwrapped.len(),
        };
    }

    if surrounds(text, &range, inline) {
        let outer = range.start - m..range.end + m;
        let select = match cursor_offset {
            Some(offset) => {
                let at = outer.start + offset;
                at..at
            }
            None => outer.start..outer.start + inner.len(),
        };
        return Edit {
            range: outer,
            insert: inner.to_string(),
            select,
        };
    }

    let select = match cursor_offset {
        Some(offset) => {
            let at = range.start + m + offset;
            at..at
        }
        None => range.start + m..range.end + m,
    };
    Edit {
        insert: format!("{marker}{inner}{marker}"),
        range,
        select,
    }
}

/// `[text](url)`: the selection becomes the text with `url` selected to type
/// over; a selected URL becomes the target with the text left to fill in.
pub fn link(text: &str, selection: Range<usize>) -> Edit {
    let selected = &text[selection.clone()];
    let start = selection.start;
    if selected.is_empty() {
        return Edit {
            range: selection,
            insert: "[](url)".into(),
            select: start + 1..start + 1,
        };
    }
    let looks_like_url = ["http://", "https://", "mailto:", "www."]
        .iter()
        .any(|p| selected.starts_with(p))
        && !selected.contains(char::is_whitespace);
    if looks_like_url {
        return Edit {
            insert: format!("[]({selected})"),
            range: selection,
            select: start + 1..start + 1,
        };
    }
    let url_start = start + selected.len() + 3;
    Edit {
        insert: format!("[{selected}](url)"),
        range: selection,
        select: url_start..url_start + 3,
    }
}

/// The parts of a line before its content.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prefix {
    indent: String,
    kind: Kind,
    /// Byte length of indent + marker, i.e. where the content starts.
    len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Plain,
    Heading(usize),
    Bullet(char),
    Numbered(u64, char),
    Task(char, bool),
    Quote,
}

fn parse_prefix(line: &str) -> Prefix {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = line[..indent_len].to_string();
    let rest = &line[indent_len..];
    let done = |kind, marker_len: usize| Prefix {
        indent: indent.clone(),
        kind,
        len: indent_len + marker_len,
    };

    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && indent_len < 4 {
        let after = &rest[hashes..];
        if after.is_empty() || after.starts_with(' ') {
            return done(Kind::Heading(hashes), hashes + after.len().min(1));
        }
    }
    if let Some(after) = rest.strip_prefix('>') {
        return done(Kind::Quote, 1 + usize::from(after.starts_with(' ')));
    }
    let mut chars = rest.chars();
    if let Some(bullet @ ('-' | '*' | '+')) = chars.next() {
        let after = &rest[1..];
        if after.starts_with(' ') || after.is_empty() {
            let body = after.strip_prefix(' ').unwrap_or(after);
            for (mark, checked) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
                if let Some(tail) = body.strip_prefix(mark)
                    && (tail.is_empty() || tail.starts_with(' '))
                {
                    let len = 2 + mark.len() + usize::from(tail.starts_with(' '));
                    return done(Kind::Task(bullet, checked), len);
                }
            }
            return done(Kind::Bullet(bullet), 1 + usize::from(!after.is_empty()));
        }
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits)
        && let Some(delim @ ('.' | ')')) = rest[digits..].chars().next()
    {
        let after = &rest[digits + 1..];
        if after.is_empty() || after.starts_with(' ') {
            let n = rest[..digits].parse().unwrap_or(1);
            return done(Kind::Numbered(n, delim), digits + 1 + usize::from(!after.is_empty()));
        }
    }
    done(Kind::Plain, 0)
}

fn is_list(kind: &Kind) -> bool {
    matches!(kind, Kind::Bullet(_) | Kind::Numbered(..) | Kind::Task(..))
}

/// The lines `selection` touches, as (start, end) byte ranges without their
/// newline. A selection ending right at a line start leaves that line out.
fn lines_of(text: &str, selection: &Range<usize>) -> Vec<Range<usize>> {
    let first = text[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    let mut last_end = selection.end;
    if selection.end > selection.start && text[..selection.end].ends_with('\n') {
        last_end -= 1;
    }
    let last = text[last_end..]
        .find('\n')
        .map_or(text.len(), |i| last_end + i);
    let mut lines = Vec::new();
    let mut start = first;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let end = end.min(last.max(start));
        lines.push(start..end);
        if end >= last {
            break;
        }
        start = end + 1;
    }
    lines
}

/// Rewrite each touched line's prefix with `rewrite(index, prefix)` (which
/// returns the new prefix), keeping content, and map the selection across.
fn rewrite_lines(
    text: &str,
    selection: Range<usize>,
    mut rewrite: impl FnMut(usize, &Prefix, &str) -> Option<String>,
) -> Edit {
    let lines = lines_of(text, &selection);
    let block = lines[0].start..lines[lines.len() - 1].end;
    let mut out = String::new();
    // (old line start, old prefix len, new line start, new prefix len)
    let mut shifts = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let content = &text[line.clone()];
        let prefix = parse_prefix(content);
        let new_start = block.start + out.len();
        match rewrite(i, &prefix, &content[prefix.len..]) {
            Some(new_prefix) => {
                shifts.push((line.start, prefix.len, new_start, new_prefix.len()));
                out.push_str(&new_prefix);
                out.push_str(&content[prefix.len..]);
            }
            None => {
                shifts.push((line.start, 0, new_start, 0));
                out.push_str(content);
            }
        }
    }
    let map = |p: usize, whole_line: bool| {
        let &(old, old_len, new, new_len) = shifts
            .iter()
            .rev()
            .find(|(old, ..)| *old <= p)
            .unwrap_or(&shifts[0]);
        // A selection that starts a line keeps the line's new marker in it.
        if whole_line && p == old {
            return new;
        }
        new + new_len + p.saturating_sub(old + old_len)
    };
    let whole = !selection.is_empty();
    Edit {
        select: map(selection.start, whole)..map(selection.end, false),
        range: block,
        insert: out,
    }
}

/// Headings, lists, task lists and quotes, applied to every selected line.
/// Applying a format all the lines already have removes it.
pub fn toggle_block(text: &str, selection: Range<usize>, block: Block) -> Edit {
    let lines = lines_of(text, &selection);
    let prefixes: Vec<_> = lines
        .iter()
        .map(|l| parse_prefix(&text[l.clone()]))
        .collect();
    let blank = |i: usize| text[lines[i].clone()].trim().is_empty() && lines.len() > 1;
    let all = |test: &dyn Fn(&Kind) -> bool| {
        prefixes
            .iter()
            .enumerate()
            .all(|(i, p)| blank(i) || test(&p.kind))
    };
    match block {
        Block::Heading(level) => {
            let off = level == 0 || all(&|k| *k == Kind::Heading(level));
            rewrite_lines(text, selection, |_, prefix, _| {
                let indent = if matches!(prefix.kind, Kind::Heading(_)) {
                    String::new()
                } else {
                    prefix.indent.clone()
                };
                let rest = match &prefix.kind {
                    // Keep list and quote markers; only headings are replaced.
                    Kind::Heading(_) | Kind::Plain => String::new(),
                    _ => return None,
                };
                Some(if off {
                    indent + &rest
                } else {
                    format!("{indent}{} ", "#".repeat(level.min(6)))
                })
            })
        }
        Block::Bullet | Block::Numbered | Block::Task | Block::Quote => {
            let target: &dyn Fn(&Kind) -> bool = match block {
                Block::Bullet => &|k| matches!(k, Kind::Bullet(_)),
                Block::Numbered => &|k| matches!(k, Kind::Numbered(..)),
                Block::Task => &|k| matches!(k, Kind::Task(..)),
                _ => &|k| *k == Kind::Quote,
            };
            let off = all(target);
            let mut number = 0;
            rewrite_lines(text, selection, |i, prefix, _| {
                if blank(i) {
                    return None;
                }
                let indent = prefix.indent.clone();
                if off {
                    return Some(indent);
                }
                number += 1;
                Some(match block {
                    Block::Bullet => format!("{indent}- "),
                    Block::Numbered => format!("{indent}{number}. "),
                    Block::Task => format!("{indent}- [ ] "),
                    _ => format!("{indent}> "),
                })
            })
        }
    }
}

/// Tick or untick task items; lines that are not tasks become open tasks.
pub fn toggle_task_done(text: &str, selection: Range<usize>) -> Edit {
    let lines = lines_of(text, &selection);
    let prefixes: Vec<_> = lines
        .iter()
        .map(|l| parse_prefix(&text[l.clone()]))
        .collect();
    // Mixed selections all become done; all-done ones all reopen.
    let check = !prefixes
        .iter()
        .filter(|p| matches!(p.kind, Kind::Task(..)))
        .all(|p| matches!(p.kind, Kind::Task(_, true)));
    rewrite_lines(text, selection, |_, prefix, content| {
        if content.trim().is_empty() && !matches!(prefix.kind, Kind::Task(..)) {
            return None;
        }
        let bullet = match prefix.kind {
            Kind::Task(bullet, _) | Kind::Bullet(bullet) => bullet,
            _ => '-',
        };
        let mark = match prefix.kind {
            Kind::Task(..) if check => "[x]",
            Kind::Task(..) => "[ ]",
            _ => "[ ]",
        };
        Some(format!("{}{bullet} {mark} ", prefix.indent))
    })
}

/// Wrap the selected lines in a ``` fence, or remove the fence when the
/// selection starts and ends on one. With nothing selected, open an empty one.
pub fn toggle_code_block(text: &str, selection: Range<usize>) -> Edit {
    let lines = lines_of(text, &selection);
    let first = lines[0].clone();
    let last = lines[lines.len() - 1].clone();
    let is_fence = |l: &Range<usize>| text[l.clone()].trim_start().starts_with("```");
    if lines.len() >= 2 && is_fence(&first) && is_fence(&last) {
        let inner = &text[(first.end + 1).min(last.start)..last.start];
        let inner = inner.strip_suffix('\n').unwrap_or(inner);
        return Edit {
            range: first.start..last.end,
            insert: inner.to_string(),
            select: first.start..first.start + inner.len(),
        };
    }
    let body = &text[first.start..last.end];
    if selection.is_empty() && body.trim().is_empty() {
        return Edit {
            range: first.start..last.end,
            insert: "```\n\n```".into(),
            select: first.start + 4..first.start + 4,
        };
    }
    Edit {
        range: first.start..last.end,
        insert: format!("```\n{body}\n```"),
        select: first.start + 4..first.start + 4 + body.len(),
    }
}

/// Enter inside a list, task list or quote: continue it on the next line, or
/// end it when the current item is empty. `None` for an ordinary newline.
pub fn continue_on_enter(text: &str, cursor: usize) -> Option<Edit> {
    let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
    let line = &text[line_start..line_end];
    let prefix = parse_prefix(line);
    if !(is_list(&prefix.kind) || prefix.kind == Kind::Quote) || cursor < line_start + prefix.len {
        return None;
    }
    if line[prefix.len..].trim().is_empty() {
        // An empty item ends the list: clear its marker, stay on the line.
        // A nested one steps out a level instead, keeping its marker.
        let insert = if is_list(&prefix.kind) && !prefix.indent.is_empty() {
            let width = prefix.indent.len().min(marker_width(&prefix));
            format!("{}{}", &prefix.indent[width..], &line[prefix.indent.len()..prefix.len])
        } else {
            String::new()
        };
        let at = line_start + insert.len();
        return Some(Edit {
            range: line_start..line_end,
            insert,
            select: at..at,
        });
    }
    let next = format!("\n{}{}", prefix.indent, next_marker(&prefix));
    let at = cursor + next.len();
    Some(Edit {
        range: cursor..cursor,
        insert: next,
        select: at..at,
    })
}

fn next_marker(prefix: &Prefix) -> String {
    match prefix.kind {
        Kind::Bullet(b) => format!("{b} "),
        Kind::Numbered(n, d) => format!("{}{d} ", n + 1),
        Kind::Task(b, _) => format!("{b} [ ] "),
        Kind::Quote => "> ".into(),
        _ => String::new(),
    }
}

/// Indentation a nested item needs under this one: its marker's width.
fn marker_width(prefix: &Prefix) -> usize {
    match prefix.kind {
        Kind::Numbered(n, _) => n.to_string().len() + 2,
        _ => 2,
    }
}

/// Tab / Shift-Tab on list items: indent or outdent every selected item by
/// one nesting level. `None` when a selected line is not a list item.
pub fn indent_list(text: &str, selection: Range<usize>, outdent: bool) -> Option<Edit> {
    let lines = lines_of(text, &selection);
    let prefixes: Vec<_> = lines
        .iter()
        .map(|l| parse_prefix(&text[l.clone()]))
        .collect();
    if !prefixes.iter().all(|p| is_list(&p.kind)) {
        return None;
    }
    Some(rewrite_lines(text, selection, |_, prefix, _| {
        let width = marker_width(prefix);
        let indent = if outdent {
            let mut indent = prefix.indent.clone();
            if indent.starts_with('\t') {
                indent.remove(0);
            } else {
                let spaces = indent.chars().take_while(|&c| c == ' ').count().min(width);
                indent.drain(..spaces);
            }
            indent
        } else {
            format!("{}{}", " ".repeat(width), prefix.indent)
        };
        let marker = match prefix.kind {
            Kind::Bullet(b) => format!("{b} "),
            Kind::Numbered(n, d) => format!("{n}{d} "),
            Kind::Task(b, done) => format!("{b} [{}] ", if done { 'x' } else { ' ' }),
            _ => String::new(),
        };
        Some(format!("{indent}{marker}"))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply `edit` to `text`, marking the resulting selection with `|` (a
    /// cursor) or `[`..`]` (a selection).
    fn show(text: &str, edit: &Edit) -> String {
        let mut out = String::new();
        out.push_str(&text[..edit.range.start]);
        out.push_str(&edit.insert);
        out.push_str(&text[edit.range.end..]);
        let s = edit.select.clone();
        if s.is_empty() {
            out.insert(s.start, '|');
        } else {
            out.insert(s.end, ']');
            out.insert(s.start, '[');
        }
        out
    }

    /// `text` written with the selection marked the same way.
    fn parse(marked: &str) -> (String, Range<usize>) {
        if let Some(at) = marked.find('|') {
            return (marked.replacen('|', "", 1), at..at);
        }
        let start = marked.find('[').unwrap();
        let end = marked.find(']').unwrap() - 1;
        (marked.replacen('[', "", 1).replacen(']', "", 1), start..end)
    }

    fn inline(marked: &str, kind: Inline) -> String {
        let (text, sel) = parse(marked);
        show(&text, &toggle_inline(&text, sel, kind))
    }

    fn block(marked: &str, kind: Block) -> String {
        let (text, sel) = parse(marked);
        show(&text, &toggle_block(&text, sel, kind))
    }

    #[test]
    fn inline_formats_wrap_and_unwrap() {
        assert_eq!(inline("a [word] b", Inline::Bold), "a **[word]** b");
        assert_eq!(inline("a **[word]** b", Inline::Bold), "a [word] b");
        assert_eq!(inline("a [**word**] b", Inline::Bold), "a [word] b");
        assert_eq!(inline("a [word ]b", Inline::Bold), "a **[word]** b");
        assert_eq!(inline("a wo|rd b", Inline::Italic), "a *wo|rd* b");
        assert_eq!(inline("a *wo|rd* b", Inline::Italic), "a wo|rd b");
        assert_eq!(inline("a | b", Inline::Bold), "a **|** b");
        assert_eq!(inline("a **|** b", Inline::Bold), "a | b");
        // Italic inside bold and bold inside italic are told apart.
        assert_eq!(inline("**[x]**", Inline::Italic), "***[x]***");
        assert_eq!(inline("***[x]***", Inline::Italic), "**[x]**");
        assert_eq!(inline("***[x]***", Inline::Bold), "*[x]*");
        assert_eq!(inline("[x]", Inline::Strikethrough), "~~[x]~~");
        assert_eq!(inline("`[x]`", Inline::Code), "[x]");
    }

    #[test]
    fn links_select_what_is_left_to_type() {
        let (text, sel) = parse("see [docs] now");
        assert_eq!(show(&text, &link(&text, sel)), "see [docs]([url]) now");
        let (text, sel) = parse("[https://a.io]");
        assert_eq!(show(&text, &link(&text, sel)), "[|](https://a.io)");
        let (text, sel) = parse("x | y");
        assert_eq!(show(&text, &link(&text, sel)), "x [|](url) y");
    }

    #[test]
    fn headings_set_switch_and_clear() {
        assert_eq!(block("Ti|tle", Block::Heading(2)), "## Ti|tle");
        assert_eq!(block("## Ti|tle", Block::Heading(3)), "### Ti|tle");
        assert_eq!(block("## Ti|tle", Block::Heading(2)), "Ti|tle");
        assert_eq!(block("# Ti|tle", Block::Heading(0)), "Ti|tle");
    }

    #[test]
    fn lists_convert_number_and_toggle() {
        assert_eq!(block("[a\nb]", Block::Bullet), "[- a\n- b]");
        assert_eq!(block("[- a\n- b]", Block::Bullet), "[a\nb]");
        assert_eq!(block("[- a\n\n- b]", Block::Numbered), "[1. a\n\n2. b]");
        assert_eq!(block("  it|em", Block::Task), "  - [ ] it|em");
        assert_eq!(block("- [ ] it|em", Block::Task), "it|em");
        assert_eq!(block("[a\nb]", Block::Quote), "[> a\n> b]");
        assert_eq!(block("> |a", Block::Quote), "|a");
    }

    #[test]
    fn tasks_tick_and_reopen() {
        let (text, sel) = parse("- [ ] a|");
        assert_eq!(show(&text, &toggle_task_done(&text, sel)), "- [x] a|");
        let (text, sel) = parse("- [x] a|");
        assert_eq!(show(&text, &toggle_task_done(&text, sel)), "- [ ] a|");
        let (text, sel) = parse("plain|");
        assert_eq!(show(&text, &toggle_task_done(&text, sel)), "- [ ] plain|");
    }

    #[test]
    fn code_blocks_fence_and_unfence() {
        let (text, sel) = parse("|");
        assert_eq!(show(&text, &toggle_code_block(&text, sel)), "```\n|\n```");
        let (text, sel) = parse("[a\nb]");
        assert_eq!(show(&text, &toggle_code_block(&text, sel)), "```\n[a\nb]\n```");
        let (text, sel) = parse("[```\na\n```]");
        assert_eq!(show(&text, &toggle_code_block(&text, sel)), "[a]");
    }

    fn enter(marked: &str) -> Option<String> {
        let (text, sel) = parse(marked);
        continue_on_enter(&text, sel.start).map(|edit| show(&text, &edit))
    }

    #[test]
    fn enter_continues_and_ends_lists() {
        assert_eq!(enter("- a|").as_deref(), Some("- a\n- |"));
        assert_eq!(enter("* a|").as_deref(), Some("* a\n* |"));
        assert_eq!(enter("9. a|").as_deref(), Some("9. a\n10. |"));
        assert_eq!(enter("- [x] a|").as_deref(), Some("- [x] a\n- [ ] |"));
        assert_eq!(enter("> a|").as_deref(), Some("> a\n> |"));
        assert_eq!(enter("  - a|").as_deref(), Some("  - a\n  - |"));
        // Empty items end the list, or step out one level when nested.
        assert_eq!(enter("- a\n- |").as_deref(), Some("- a\n|"));
        assert_eq!(enter("- [ ] |").as_deref(), Some("|"));
        assert_eq!(enter("- a\n  - |").as_deref(), Some("- a\n- |"));
        // Plain text and the cursor inside a marker keep the normal Enter.
        assert_eq!(enter("plain|"), None);
        assert_eq!(enter("-| a"), None);
    }

    #[test]
    fn tab_nests_list_items_only() {
        let (text, sel) = parse("- a\n- b|");
        assert_eq!(
            show(&text, &indent_list(&text, sel, false).unwrap()),
            "- a\n  - b|"
        );
        let (text, sel) = parse("1. a\n   1. b|");
        assert_eq!(
            show(&text, &indent_list(&text, sel, true).unwrap()),
            "1. a\n1. b|"
        );
        let (text, sel) = parse("plain|");
        assert!(indent_list(&text, sel, false).is_none());
    }
}
