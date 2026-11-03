use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seg {
    pub leaf: Range<usize>,
    pub src: Range<usize>,
    pub linear: bool,
    pub line_end: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LeafMap {
    pub key: u64,
    pub text: String,
    pub segs: Vec<Seg>,
    pub empty_src: usize,
}

impl LeafMap {
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn src_range(&self) -> Range<usize> {
        match (self.segs.first(), self.segs.last()) {
            (Some(first), Some(last)) => first.src.start..last.src.end,
            _ => self.empty_src..self.empty_src,
        }
    }

    pub fn leaf_to_src(&self, index: usize) -> usize {
        let Some(first) = self.segs.first() else {
            return self.empty_src;
        };
        if index <= first.leaf.start {
            return first.src.start;
        }
        let after_newline = self.text[..index.min(self.text.len())].ends_with('\n');
        for (ix, seg) in self.segs.iter().enumerate() {
            if (after_newline || seg.line_end)
                && index == seg.leaf.end
                && let Some(next) = self.segs.get(ix + 1)
                && next.leaf.start == index
            {
                return next.src.start;
            }
            if index > seg.leaf.start && index <= seg.leaf.end {
                return if seg.linear {
                    seg.src.start + (index - seg.leaf.start)
                } else if index == seg.leaf.end {
                    seg.src.end
                } else {
                    seg.src.start
                };
            }
        }
        self.segs.last().map_or(self.empty_src, |seg| seg.src.end)
    }

    pub fn src_to_leaf(&self, src: usize) -> usize {
        let Some(first) = self.segs.first() else {
            return 0;
        };
        if src <= first.src.start {
            return first.leaf.start;
        }
        let mut best = first.leaf.start;
        for seg in &self.segs {
            if src < seg.src.start {
                break;
            }
            if src <= seg.src.end {
                return if seg.linear {
                    let index = seg.leaf.start + (src - seg.src.start);
                    floor_char_boundary(&self.text, index.min(seg.leaf.end))
                } else if src == seg.src.start {
                    seg.leaf.start
                } else {
                    seg.leaf.end
                };
            }
            best = seg.leaf.end;
        }
        best
    }

    pub fn is_stop(&self, index: usize) -> bool {
        !self
            .segs
            .iter()
            .any(|s| !s.linear && s.leaf.start < index && index < s.leaf.end)
    }

    pub fn snap_index(&self, index: usize, forward: bool) -> usize {
        match self
            .segs
            .iter()
            .find(|s| !s.linear && s.leaf.start < index && index < s.leaf.end)
        {
            Some(s) if forward => s.leaf.end,
            Some(s) => s.leaf.start,
            None => index,
        }
    }

    pub fn src_distance(&self, src: usize) -> usize {
        let range = self.src_range();
        if src < range.start {
            range.start - src
        } else {
            src.saturating_sub(range.end)
        }
    }
}

pub fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn entity_len(text: &str) -> Option<usize> {
    let body = text.strip_prefix('&')?;
    let end = body.find(';')?;
    let name = &body[..end];
    let valid = !name.is_empty()
        && end <= 32
        && (name.chars().all(|c| c.is_ascii_alphanumeric())
            || name.strip_prefix('#').is_some_and(|n| {
                n.strip_prefix(['x', 'X']).map_or(
                    !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()),
                    |h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()),
                )
            }));
    valid.then_some(end + 2)
}

#[derive(Default)]
pub struct LeafBuilder {
    pub text: String,
    pub segs: Vec<Seg>,
}

impl LeafBuilder {
    fn push_seg(&mut self, seg: Seg) {
        if let Some(last) = self.segs.last_mut()
            && last.linear
            && seg.linear
            && last.leaf.end == seg.leaf.start
            && last.src.end == seg.src.start
        {
            last.leaf.end = seg.leaf.end;
            last.src.end = seg.src.end;
            return;
        }
        self.segs.push(seg);
    }

    pub fn push_aligned(&mut self, value: &str, source: &str, base: usize) {
        self.align(value, source, base, false);
    }

    pub fn push_prose(&mut self, value: &str, source: &str, base: usize) {
        self.align(value, source, base, true);
    }

    fn align(&mut self, value: &str, source: &str, base: usize, soft_breaks: bool) {
        let mut j = 0;
        let bytes = source.as_bytes();
        let mut chars = value.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let leaf_start = self.text.len();
            if soft_breaks && (c == '\n' || c == '\r') {
                if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
                    chars.next();
                }
                self.text.push(' ');
                while j < source.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                let end = if source[j..].starts_with("\r\n") {
                    j + 2
                } else if j < source.len() && matches!(bytes[j], b'\n' | b'\r') {
                    j + 1
                } else {
                    j
                };
                self.push_seg(Seg {
                    leaf: leaf_start..self.text.len(),
                    src: base + j..base + end,
                    linear: false,
                    line_end: true,
                });
                j = end;
                continue;
            }
            self.text.push(c);
            let leaf = leaf_start..self.text.len();
            loop {
                if j >= source.len() {
                    self.push_seg(Seg {
                        leaf: leaf.clone(),
                        src: base + source.len()..base + source.len(),
                        linear: false,
                        line_end: false,
                    });
                    break;
                }
                let rest = &source[j..];
                if bytes[j] == b'&'
                    && let Some(end) = entity_len(rest)
                    && !value[i..].starts_with(&rest[..end])
                {
                    self.push_seg(Seg {
                        leaf: leaf.clone(),
                        src: base + j..base + j + end,
                        linear: false,
                        line_end: false,
                    });
                    j += end;
                    break;
                }
                if rest.starts_with(c) {
                    self.push_seg(Seg {
                        leaf: leaf.clone(),
                        src: base + j..base + j + c.len_utf8(),
                        linear: true,
                        line_end: false,
                    });
                    j += c.len_utf8();
                    break;
                }
                if bytes[j] == b'\\' && rest[1..].starts_with(c) {
                    self.push_seg(Seg {
                        leaf: leaf.clone(),
                        src: base + j..base + j + 1 + c.len_utf8(),
                        linear: false,
                        line_end: false,
                    });
                    j += 1 + c.len_utf8();
                    break;
                }
                j += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
    }

    pub fn push_atomic(&mut self, text: &str, src: Range<usize>) {
        let start = self.text.len();
        self.text.push_str(text);
        self.push_seg(Seg {
            leaf: start..self.text.len(),
            src,
            linear: false,
            line_end: text == "\n",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation as _;

    fn leaf(value: &str, source: &str) -> LeafMap {
        let mut b = LeafBuilder::default();
        b.push_aligned(value, source, 0);
        LeafMap {
            key: 0,
            text: b.text,
            segs: b.segs,
            empty_src: 0,
        }
    }

    #[test]
    fn plain_text_is_one_linear_run() {
        let map = leaf("héllo 世界 👍🏽", "héllo 世界 👍🏽");
        assert_eq!(map.segs.len(), 1);
        assert!(map.segs[0].linear);
    }

    #[test]
    fn soft_breaks_render_as_spaces_mapped_to_the_line_ending() {
        let mut b = LeafBuilder::default();
        b.push_prose("one\ntwo", "one  \r\n> two", 0);
        let map = LeafMap {
            key: 0,
            text: b.text,
            segs: b.segs,
            empty_src: 0,
        };
        assert_eq!(map.text, "one two");
        assert_eq!(map.leaf_to_src(3), 3);
        assert_eq!(map.leaf_to_src(4), 9, "after the break: after the prefix");
        assert_eq!(
            map.src_to_leaf(5),
            3,
            "the line ending maps to the space as a whole"
        );
    }

    #[test]
    fn escapes_and_entities_map_as_a_whole() {
        let map = leaf("a*b&c", r"a\*b&amp;c");
        assert_eq!(map.leaf_to_src(1), 1);
        assert_eq!(map.leaf_to_src(2), 3);
        assert_eq!(map.leaf_to_src(4), 9);
        assert_eq!(map.src_to_leaf(2), 2, "inside an escape snaps to its end");
    }

    #[test]
    fn container_prefixes_are_skipped() {
        let map = leaf("one\ntwo", "one\n> two");
        assert_eq!(map.leaf_to_src(4), 6, "a new line starts after the prefix");
        assert_eq!(map.src_to_leaf(5), 4);
    }

    #[test]
    fn leaf_to_src_to_leaf_is_identity_at_grapheme_boundaries() {
        let cases = [
            ("नमस्ते दुनिया। अगला वाक्य।", "नमस्ते दुनिया। अगला वाक्य।"),
            ("中文 and 👍🏽 x", "中文 and 👍🏽 x"),
            ("a*b&c", r"a\*b&amp;c"),
            ("one\ntwo", "one\n> two"),
        ];
        for (value, source) in cases {
            let map = leaf(value, source);
            let boundaries = value
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([value.len()]);
            for i in boundaries {
                assert_eq!(map.src_to_leaf(map.leaf_to_src(i)), i, "{value:?} at {i}");
            }
        }
    }
}
