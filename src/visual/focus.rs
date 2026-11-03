use std::ops::Range;

const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "mt", "vs", "etc", "e.g", "i.e", "cf",
    "fig", "no", "vol", "ch", "approx", "inc", "ltd", "co", "jan", "feb", "mar", "apr", "jun",
    "jul", "aug", "sep", "sept", "oct", "nov", "dec", "p", "pp", "ed", "eds", "al",
];

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…' | '।' | '॥' | '。' | '！' | '？')
}

fn is_closing(c: char) -> bool {
    matches!(c, '"' | '\'' | '”' | '’' | ')' | ']' | '»' | '*' | '_')
}

fn ends_sentence(text: &str, at: usize, c: char, after: &str) -> bool {
    if c != '.' {
        return true;
    }
    if after
        .trim_start()
        .chars()
        .next()
        .is_some_and(|next| next.is_lowercase())
    {
        return false;
    }
    let word: String = text[..at]
        .chars()
        .rev()
        .take_while(|c| !c.is_whitespace() && *c != '(' && *c != '"')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    if word.is_empty() {
        return true;
    }
    let mut letters = word.chars();
    if let (Some(first), None) = (letters.next(), letters.next())
        && first.is_alphabetic()
        && first.is_uppercase()
    {
        return false;
    }
    !ABBREVIATIONS.contains(&word.to_lowercase().as_str())
}

pub fn sentences(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !is_terminal(c) {
            continue;
        }
        let mut end = i + c.len_utf8();
        while let Some(&(j, next)) = chars.peek() {
            if is_terminal(next) || is_closing(next) {
                end = j + next.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        let at_space = chars.peek().is_none_or(|&(_, next)| next.is_whitespace());
        let wide = matches!(c, '。' | '！' | '？' | '।' | '॥');
        if (at_space || wide) && ends_sentence(text, i, c, &text[end..]) {
            while let Some(&(j, next)) = chars.peek() {
                if next.is_whitespace() {
                    end = j + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(start..end);
            start = end;
        }
    }
    if start < text.len() || out.is_empty() {
        out.push(start..text.len());
    }
    out
}

pub fn sentence_at(text: &str, index: usize) -> Range<usize> {
    let all = sentences(text);
    let found = all
        .iter()
        .position(|s| index < s.end)
        .unwrap_or(all.len() - 1);
    let range = all[found].clone();
    if index == range.start && found > 0 && index == text.len() {
        return all[found - 1].clone();
    }
    let trimmed = text[range.clone()].trim_end().len();
    range.start..range.start + trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        sentences(text)
            .into_iter()
            .map(|r| text[r].trim_end())
            .collect()
    }

    #[test]
    fn splits_sentences_and_keeps_abbreviations() {
        assert_eq!(
            split("Dr. Smith arrived at 3.30 p.m. today. He left! Did she? Yes."),
            [
                "Dr. Smith arrived at 3.30 p.m. today.",
                "He left!",
                "Did she?",
                "Yes."
            ]
        );
        assert_eq!(
            split("See e.g. the map. J. R. Tolkien wrote it."),
            ["See e.g. the map.", "J. R. Tolkien wrote it."]
        );
        assert_eq!(
            split("He said \"Stop.\" Then he went (quietly.) Done"),
            ["He said \"Stop.\"", "Then he went (quietly.)", "Done"]
        );
        assert_eq!(split("Wait... what?! Fine"), ["Wait... what?!", "Fine"]);
    }

    #[test]
    fn splits_devanagari_and_cjk() {
        assert_eq!(
            split("नमस्ते दुनिया। आप कैसे हैं॥ ठीक"),
            ["नमस्ते दुनिया।", "आप कैसे हैं॥", "ठीक"]
        );
        assert_eq!(split("你好。我很好！"), ["你好。", "我很好！"]);
    }

    #[test]
    fn finds_the_sentence_at_a_caret() {
        let text = "One two. Three four. Five";
        assert_eq!(&text[sentence_at(text, 2)], "One two.");
        assert_eq!(&text[sentence_at(text, 9)], "Three four.");
        assert_eq!(&text[sentence_at(text, text.len())], "Five");
        let ended = "One. Two.";
        assert_eq!(&ended[sentence_at(ended, ended.len())], "Two.");
        assert_eq!(sentence_at("", 0), 0..0);
    }
}
