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
    /// Inline HTML formatting: `<mark>`, `<b>`, `<u>`…
    Html,
}

impl Wrap {
    /// The delimiter written around a new run of this kind.
    pub fn delimiter(self) -> &'static str {
        match self {
            Wrap::Strong => "**",
            Wrap::Emphasis => "*",
            Wrap::Delete => "~~",
            Wrap::Code => "`",
            Wrap::Link | Wrap::Wiki | Wrap::Html => "",
        }
    }
}

/// A highlight: its Markdown, its tags, and its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkAt {
    pub source: Range<usize>,
    pub open: Range<usize>,
    pub close: Range<usize>,
    pub color: gpui_kit::Hsla,
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
    /// Inline math, drawn as one unit: where it's drawn, its Markdown, and
    /// its TeX.
    pub maths: Vec<(Range<usize>, Range<usize>, String)>,
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

thread_local! {
    /// The note's link definitions (`[id]: url`), for reference links, while
    /// a page is read.
    static DEFINITIONS: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
    static FOOTNOTES: std::cell::RefCell<std::collections::HashMap<String, usize>> = Default::default();
}

/// Run `f` with the note's link definitions known to reference links, and
/// its footnotes' numbers (by identifier) to footnote marks.
pub fn with_definitions<R>(
    definitions: std::collections::HashMap<String, String>,
    footnotes: std::collections::HashMap<String, usize>,
    f: impl FnOnce() -> R,
) -> R {
    DEFINITIONS.with(|d| *d.borrow_mut() = definitions);
    FOOTNOTES.with(|d| *d.borrow_mut() = footnotes);
    let result = f();
    DEFINITIONS.with(|d| d.borrow_mut().clear());
    FOOTNOTES.with(|d| d.borrow_mut().clear());
    result
}

/// A footnote's number as drawn: its order in the note, else its label.
pub fn footnote_number(identifier: &str, label: Option<&str>) -> String {
    match FOOTNOTES.with(|d| d.borrow().get(&identifier.to_lowercase()).copied()) {
        Some(n) => n.to_string(),
        None => label.unwrap_or(identifier).to_string(),
    }
}

/// `text` in superscript letters where Unicode has them.
fn superscript(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴', '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
            '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾', 'n' => 'ⁿ', 'i' => 'ⁱ',
            c => c,
        })
        .collect()
}

fn subscript(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄', '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
            '+' => '₊', '-' => '₋', '=' => '₌', '(' => '₍', ')' => '₎',
            c => c,
        })
        .collect()
}

/// TeX drawn as text: Greek letters, common symbols, simple powers and
/// indices, `\frac{a}{b}` as a⁄b. What it can't draw stays as written.
pub fn tex_text(tex: &str) -> String {
    const SYMBOLS: &[(&str, &str)] = &[
        ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"), ("epsilon", "ε"), ("zeta", "ζ"), ("eta", "η"),
        ("theta", "θ"), ("iota", "ι"), ("kappa", "κ"), ("lambda", "λ"), ("mu", "μ"), ("nu", "ν"), ("xi", "ξ"),
        ("pi", "π"), ("rho", "ρ"), ("sigma", "σ"), ("tau", "τ"), ("phi", "φ"), ("chi", "χ"), ("psi", "ψ"),
        ("omega", "ω"), ("Gamma", "Γ"), ("Delta", "Δ"), ("Theta", "Θ"), ("Lambda", "Λ"), ("Pi", "Π"), ("Sigma", "Σ"),
        ("Phi", "Φ"), ("Psi", "Ψ"), ("Omega", "Ω"), ("times", "×"), ("cdot", "·"), ("div", "÷"), ("pm", "±"),
        ("mp", "∓"), ("le", "≤"), ("leq", "≤"), ("ge", "≥"), ("geq", "≥"), ("neq", "≠"), ("ne", "≠"),
        ("approx", "≈"), ("equiv", "≡"), ("sim", "∼"), ("infty", "∞"), ("sum", "∑"), ("prod", "∏"), ("int", "∫"),
        ("partial", "∂"), ("nabla", "∇"), ("sqrt", "√"), ("to", "→"), ("rightarrow", "→"), ("leftarrow", "←"),
        ("Rightarrow", "⇒"), ("Leftarrow", "⇐"), ("leftrightarrow", "↔"), ("in", "∈"), ("notin", "∉"),
        ("subset", "⊂"), ("subseteq", "⊆"), ("cup", "∪"), ("cap", "∩"), ("forall", "∀"), ("exists", "∃"),
        ("emptyset", "∅"), ("ldots", "…"), ("cdots", "⋯"), ("degree", "°"), ("circ", "∘"), ("neg", "¬"),
        ("land", "∧"), ("lor", "∨"), ("quad", " "), ("qquad", "  "), (",", " "), (";", " "), ("!", ""),
    ];
    // The group after `at` (`{…}` or one letter) and where it ends.
    fn group(chars: &[char], at: usize) -> (String, usize) {
        if chars.get(at) == Some(&'{') {
            let mut depth = 0;
            for (i, c) in chars.iter().enumerate().skip(at) {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return (chars[at + 1..i].iter().collect(), i + 1);
                        }
                    }
                    _ => {}
                }
            }
            (chars[at + 1..].iter().collect(), chars.len())
        } else {
            (chars.get(at).map(|c| c.to_string()).unwrap_or_default(), (at + 1).min(chars.len()))
        }
    }
    let chars: Vec<char> = tex.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                let name: String = chars[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).collect();
                let name = if name.is_empty() { chars.get(i + 1).map(|c| c.to_string()).unwrap_or_default() } else { name };
                i += 1 + name.len();
                if name == "frac" {
                    let (top, next) = group(&chars, i);
                    let (bottom, next) = group(&chars, next);
                    out.push_str(&format!("{}⁄{}", tex_text(&top), tex_text(&bottom)));
                    i = next;
                } else if name == "text" || name == "mathrm" || name == "mathbf" || name == "operatorname" {
                    let (inner, next) = group(&chars, i);
                    out.push_str(&inner);
                    i = next;
                } else if let Some((_, symbol)) = SYMBOLS.iter().find(|(n, _)| *n == name) {
                    out.push_str(symbol);
                } else if ["{", "}", "$", "%", "&", "#", "_"].contains(&name.as_str()) {
                    out.push_str(&name);
                } else {
                    out.push('\\');
                    out.push_str(&name);
                }
            }
            '^' | '_' => {
                let (inner, next) = group(&chars, i + 1);
                let inner = tex_text(&inner);
                out.push_str(&if chars[i] == '^' { superscript(&inner) } else { subscript(&inner) });
                i = next;
            }
            '{' | '}' => i += 1,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// A drawn unit standing for all of `source` (a footnote mark, inline
/// math): the caret goes around it, and it's deleted whole.
fn push_atom(flow: &mut Flow, text: &str, source: Range<usize>, mark: TextMark) {
    let start = flow.text.len();
    flow.text.push_str(text);
    flow.chars.push(Char { shown: start..flow.text.len(), source });
    if mark != TextMark::default() {
        flow.marks.push((start..flow.text.len(), mark));
    }
}

fn walk(block: &str, base: usize, nodes: &[Node], mark: TextMark, flow: &mut Flow) -> Option<()> {
    let mut index = 0;
    while index < nodes.len() {
        let node = &nodes[index];
        index += 1;
        let position = node.position()?;
        let at = position.start.offset..position.end.offset;
        match node {
            // Inline HTML: a break, or formatting from its tag to the
            // matching closing tag, the tags hidden like `**`.
            Node::Html(html) => {
                let (closing, name, attrs) = inline_tag(&html.value)?;
                if closing {
                    return None;
                }
                if name == "br" {
                    let start = flow.text.len();
                    flow.text.push('\n');
                    flow.chars.push(Char { shown: start..start + 1, source: base + at.start..base + at.end });
                    continue;
                }
                let tag_mark = html_mark(&name, &attrs)?;
                let close = matching_close(nodes, index - 1, &name)?;
                let close_at = nodes[close].position()?;
                let mut inner_mark = mark.clone();
                inner_mark.merge(tag_mark);
                let start = flow.text.len();
                walk(block, base, &nodes[index..close], inner_mark, flow)?;
                flow.wrappers.push(Wrapper {
                    kind: Wrap::Html,
                    shown: start..flow.text.len(),
                    source: base + at.start..base + close_at.end.offset,
                    open: block[at.clone()].to_string(),
                    close: block[close_at.start.offset..close_at.end.offset].to_string(),
                });
                index = close + 1;
            }
            // `[text][id]`: a link to its definition's address.
            Node::LinkReference(reference) => {
                let url = DEFINITIONS.with(|d| d.borrow().get(&reference.identifier.to_lowercase()).cloned())?;
                let children = &reference.children;
                let mut inner_mark = mark.clone();
                inner_mark.link = Some(LinkMark { url: url.into(), identifier: None, title: None });
                let start = flow.text.len();
                walk(block, base, children, inner_mark, flow)?;
                let inner = children.first().and_then(|c| c.position()).map(|p| p.start.offset)?
                    ..children.last().and_then(|c| c.position()).map(|p| p.end.offset)?;
                flow.wrappers.push(Wrapper {
                    kind: Wrap::Link,
                    shown: start..flow.text.len(),
                    source: base + at.start..base + at.end,
                    open: block[at.start..inner.start].to_string(),
                    close: block[inner.end..at.end].to_string(),
                });
            }
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
            // `[^1]`: its number, raised.
            Node::FootnoteReference(note) => {
                let number = footnote_number(&note.identifier, note.label.as_deref());
                let shown = if number.chars().all(|c| c.is_ascii_digit()) { superscript(&number) } else { format!("[{number}]") };
                push_atom(flow, &shown, base + at.start..base + at.end, mark.clone());
            }
            // `$…$`: drawn as text, in italic; ⌘K edits its TeX.
            Node::InlineMath(math) => {
                let mut math_mark = mark.clone();
                math_mark.italic = true;
                let start = flow.text.len();
                let shown = tex_text(&math.value);
                let shown = if shown.trim().is_empty() { "∅".to_string() } else { shown };
                push_atom(flow, &shown, base + at.start..base + at.end, math_mark);
                flow.maths.push((start..flow.text.len(), base + at.start..base + at.end, math.value.clone()));
            }
            // Images: not drawn by this editor yet.
            _ => return None,
        }
    }
    Some(())
}

/// An inline HTML tag: whether it closes, its name, and its attributes.
fn inline_tag(html: &str) -> Option<(bool, String, String)> {
    let tag = html.trim().strip_prefix('<')?.strip_suffix('>')?;
    let (closing, tag) = match tag.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, tag),
    };
    let name: String = tag.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    Some((closing, name.clone(), tag[name.len()..].trim_end_matches('/').to_string()))
}

/// The marks a formatting tag gives, as the reader draws them.
fn html_mark(name: &str, attrs: &str) -> Option<TextMark> {
    let mut mark = TextMark::default();
    match name {
        "strong" | "b" => mark.bold = true,
        "em" | "i" => mark.italic = true,
        "u" => mark.underline = true,
        "s" | "del" | "strike" => mark.strikethrough = true,
        "mark" => mark.highlight = Some(mark_color(attrs)),
        _ => return None,
    }
    Some(mark)
}

/// A `<mark>`'s colour: `color`, else `style`'s `background-color`, else
/// the reader's yellow.
fn mark_color(attrs: &str) -> gpui_kit::Hsla {
    let attr = |name: &str| {
        let at = attrs.find(&format!("{name}="))? + name.len() + 1;
        let rest = &attrs[at..];
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = rest[1..].find(quote)? + 1;
        Some(rest[1..end].to_string())
    };
    let parse = |value: &str| -> Option<gpui_kit::Hsla> {
        let value = value.trim();
        if value.starts_with('#') {
            return gpui_kit::Rgba::try_from(value).ok().map(Into::into);
        }
        match value.to_ascii_lowercase().as_str() {
            "black" => Some(gpui_kit::rgb(0x000000).into()),
            "white" => Some(gpui_kit::rgb(0xffffff).into()),
            "blue" => Some(gpui_kit::rgb(0x3b82f6).into()),
            "yellow" => Some(gpui_kit::rgb(0xfacc15).into()),
            _ => None,
        }
    };
    attr("color")
        .and_then(|c| parse(&c))
        .or_else(|| {
            let style = attr("style")?;
            style.split(';').find_map(|d| {
                let (key, value) = d.split_once(':')?;
                key.trim().eq_ignore_ascii_case("background-color").then(|| parse(value)).flatten()
            })
        })
        .unwrap_or_else(|| gpui_kit::rgb(0xfef08a).into())
}

/// The sibling closing `name`'s tag opened at `open`.
fn matching_close(nodes: &[Node], open: usize, name: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, node) in nodes.iter().enumerate().skip(open + 1) {
        let Node::Html(html) = node else { continue };
        match inline_tag(&html.value) {
            Some((false, tag, _)) if tag == name => depth += 1,
            Some((true, tag, _)) if tag == name => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The character an entity (`&amp;`, `&#169;`) stands for.
fn decode_entity(entity: &str) -> Option<char> {
    let name = entity.strip_prefix('&')?.strip_suffix(';')?;
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "rarr" => '→',
        "larr" => '←',
        "uarr" => '↑',
        "darr" => '↓',
        "times" => '×',
        "deg" => '°',
        "middot" => '·',
        "bull" => '•',
        _ => return None,
    })
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
        let mut len = s.len_utf8();
        // An entity (`&amp;`): one drawn character for all of it.
        let entity = (s == '&')
            .then(|| raw[i..].find(';').filter(|n| *n <= 10))
            .flatten()
            .filter(|semi| decode_entity(&raw[i..=i + semi]) == Some(c));
        if let Some(semi) = entity {
            while source.peek().is_some_and(|(j, _)| *j <= i + semi) {
                source.next();
            }
            len = semi + 1;
        } else if s != c {
            return None;
        }
        let char_source = base + at.start + i..base + at.start + i + len;
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

    /// The next place the caret can stop after (or before) drawn position
    /// `at`: a letter on, or past a whole unit such as inline math.
    pub fn step(&self, at: usize, forward: bool) -> usize {
        if forward {
            self.chars.iter().find(|c| c.shown.end > at).map_or(self.text.len(), |c| c.shown.end)
        } else {
            self.chars.iter().rev().find(|c| c.shown.start < at).map_or(0, |c| c.shown.start)
        }
    }

    /// The inline math at (or just before) drawn position `at`: its Markdown
    /// and TeX.
    pub fn math_at(&self, at: usize) -> Option<(Range<usize>, String)> {
        self.maths
            .iter()
            .find(|(shown, ..)| shown.start <= at && at <= shown.end)
            .map(|(_, source, tex)| (source.clone(), tex.clone()))
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

    /// The inline HTML tag `tag` (`u`, `mark`…) drawn text `shown` is
    /// wholly inside: its opening and closing tags.
    pub fn tag_over(&self, shown: Range<usize>, tag: &str) -> Option<(Range<usize>, Range<usize>)> {
        let w = self
            .wrappers
            .iter()
            .filter(|w| w.kind == Wrap::Html && !w.shown.is_empty() && w.shown.start <= shown.start && shown.end <= w.shown.end)
            .filter(|w| inline_tag(&w.open).is_some_and(|(_, name, _)| name == tag))
            .min_by_key(|w| w.source.len())?;
        Some((w.source.start..w.source.start + w.open.len(), w.source.end - w.close.len()..w.source.end))
    }

    /// The delimiters to remove (last first) to clear the formatting of
    /// drawn text `shown`: every mark (bold, code, highlight, HTML…) around
    /// it or inside it. Links stay.
    pub fn clear_edits(&self, shown: Range<usize>) -> Vec<Range<usize>> {
        let mut gone: Vec<Range<usize>> = self
            .wrappers
            .iter()
            .filter(|w| !matches!(w.kind, Wrap::Link | Wrap::Wiki) && !w.shown.is_empty())
            .filter(|w| {
                (w.shown.start <= shown.start && shown.end <= w.shown.end)
                    || (shown.start <= w.shown.start && w.shown.end <= shown.end && !shown.is_empty())
            })
            .flat_map(|w| [w.source.start..w.source.start + w.open.len(), w.source.end - w.close.len()..w.source.end])
            .filter(|r| !r.is_empty())
            .collect();
        gone.sort_by_key(|r| std::cmp::Reverse(r.start));
        gone.dedup();
        gone
    }

    /// The kinds of marked runs drawn text `shown` is wholly inside.
    pub fn wraps_over(&self, shown: Range<usize>) -> Vec<Wrap> {
        self.wrappers
            .iter()
            .filter(|w| !w.shown.is_empty() && w.shown.start <= shown.start && shown.end <= w.shown.end)
            .map(|w| w.kind)
            .collect()
    }

    /// The edit that changes the case of drawn text `shown` with `case`
    /// (each letter in turn, given the letter before it), letter by letter
    /// in the Markdown so its marks stay as written. `None`: nothing changes.
    pub fn recase(
        &self,
        source: &str,
        shown: Range<usize>,
        mut case: impl FnMut(char, Option<char>) -> String,
    ) -> Option<(Range<usize>, String)> {
        let chars: Vec<&Char> = self.chars.iter().filter(|c| shown.start <= c.shown.start && c.shown.end <= shown.end).collect();
        let (first, last) = (chars.first()?, chars.last()?);
        let range = first.source.start..last.source.end;
        let mut out = String::new();
        let mut at = range.start;
        let mut before = self.text[..first.shown.start].chars().next_back();
        for c in &chars {
            let drawn = &self.text[c.shown.clone()];
            out.push_str(&source[at..c.source.start]);
            let written = &source[c.source.clone()];
            let mut letters = drawn.chars();
            match (letters.next(), letters.next()) {
                // A letter written as itself: its new case.
                (Some(letter), None) if written.chars().eq(std::iter::once(letter)) => out.push_str(&case(letter, before)),
                _ => out.push_str(written),
            }
            before = drawn.chars().next_back();
            at = c.source.end;
        }
        (out != source[range.clone()]).then_some((range, out))
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

    /// The highlight (`<mark>`) at drawn position `at` (`within`: strictly
    /// on a letter of it, as under the pointer): the whole run, its opening
    /// and closing tags, and its colour.
    pub fn mark_at(&self, at: usize, within: bool) -> Option<MarkAt> {
        let w = self
            .wrappers
            .iter()
            .filter(|w| w.kind == Wrap::Html && w.open.to_ascii_lowercase().starts_with("<mark") && !w.shown.is_empty())
            .filter(|w| if within { w.shown.start <= at && at < w.shown.end } else { w.shown.start <= at && at <= w.shown.end })
            .min_by_key(|w| w.source.len())?;
        let (_, _, attrs) = inline_tag(&w.open)?;
        Some(MarkAt {
            source: w.source.clone(),
            open: w.source.start..w.source.start + w.open.len(),
            close: w.source.end - w.close.len()..w.source.end,
            color: mark_color(&attrs),
        })
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
    fn inline_html_entities_and_references() {
        let source = "A <mark color=\"blue\">marked **bold**</mark>, <u>under</u><br>and &amp; &rarr; done";
        let f = of(source);
        assert_eq!(f.text, "A marked bold, under\nand & → done");
        // Typing inside the mark goes inside it.
        let at = f.text.find("marked").unwrap() + 3;
        assert_eq!(&source[..f.source_for(at)], "A <mark color=\"blue\">mar");
        // The entity is one letter: deleting it takes the whole `&amp;`.
        let amp = f.text.find('&').unwrap();
        assert_eq!(&source[f.delete_before(amp + 1).unwrap()], "&amp;");
        // A reference link, its address from the note's definition.
        let page = super::super::page::Page::read("See [the spec][gfm] now\n\n[gfm]: https://github.github.com/gfm/\n");
        let linked = &page.leaves[0].flow;
        assert_eq!(linked.text, "See the spec now");
        assert_eq!(linked.link_at(5).unwrap().url, "https://github.github.com/gfm/");
    }

    #[test]
    fn footnotes_and_math_are_whole_units() {
        let source = "A note.[^b] Then $E = mc^2$ and \\(not\\).\n\n[^b]: The **text**.\n";
        let page = super::super::page::Page::read(source);
        let f = &page.leaves[0].flow;
        assert_eq!(f.text, "A note.¹ Then E = mc² and (not).");
        // The caret steps over each whole, and Backspace takes it whole.
        let mark = f.text.find('¹').unwrap();
        assert_eq!(f.step(mark, true), mark + '¹'.len_utf8());
        assert_eq!(&source[f.delete_before(mark + '¹'.len_utf8()).unwrap()], "[^b]");
        let math = f.text.find('E').unwrap();
        let end = f.step(math, true);
        assert_eq!(&f.text[math..end], "E = mc²");
        assert_eq!(f.step(end, false), math);
        assert_eq!(&source[f.delete_before(end).unwrap()], "$E = mc^2$");
        let (at, tex) = f.math_at(math + 1).unwrap();
        assert_eq!((&source[at], tex.as_str()), ("$E = mc^2$", "E = mc^2"));
        // The footnote's own text is a line, numbered.
        assert_eq!(page.leaves[1].flow.text, "The text.");
        assert!(matches!(&page.blocks[1].1, super::super::page::Shape::Note { number, .. } if number == "1"));
        assert_eq!(tex_text("\\frac{a}{b} \\le \\alpha_1^{n}"), "a⁄b ≤ α₁ⁿ");
    }

    #[test]
    fn case_changes_keep_marks_as_written() {
        let source = "hello **big** w&amp;rld";
        let f = of(source);
        let title = |c: char, before: Option<char>| -> String {
            if before.is_none_or(|b| !b.is_alphanumeric()) { c.to_uppercase().collect() } else { c.to_lowercase().collect() }
        };
        let (range, text) = f.recase(source, 0..f.text.len(), title).unwrap();
        assert_eq!(format!("{}{}{}", &source[..range.start], text, &source[range.end..]), "Hello **Big** W&amp;Rld");
        assert!(f.recase(source, 0..5, |c, _| c.to_string()).is_none());
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
