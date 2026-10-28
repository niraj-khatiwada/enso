/// Count whitespace-delimited words containing a letter or number. Streaming
/// characters also work across rope chunks, without materializing the file.
pub fn word_count(text: impl Iterator<Item = char>) -> usize {
    let mut count = 0;
    let mut word = false;
    for character in text {
        if character.is_whitespace() {
            count += usize::from(word);
            word = false;
        } else if character.is_alphanumeric() {
            word = true;
        }
    }
    count + usize::from(word)
}

pub fn label(count: usize) -> String {
    let digits = count.to_string();
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    format!("{formatted} {}", if count == 1 { "word" } else { "words" })
}

#[cfg(test)]
mod tests {
    #[test]
    fn counts_unicode_words_and_ignores_markdown_markers() {
        assert_eq!(
            super::word_count("# Hello **Enso**\n- नमस्ते संसार\n\n123 🎉".chars()),
            5
        );
        assert_eq!(super::word_count("".chars()), 0);
        assert_eq!(super::label(1_234_567), "1,234,567 words");
        assert_eq!(super::label(1), "1 word");
    }
}
