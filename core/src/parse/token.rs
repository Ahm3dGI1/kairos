//! Whitespace tokenizer that keeps byte spans, so every extractor can report
//! exactly which slice of the input produced a field — and so the title can be
//! rebuilt from whatever nobody claimed.

use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Token {
    /// Lowercased, edge-punctuation-trimmed form. Used for all matching.
    pub text: String,
    /// The original slice, used to rebuild the title with its casing intact.
    pub raw: String,
    pub start: usize,
    pub end: usize,
    /// Set once an extractor claims this token.
    pub consumed: bool,
}

const EDGE_PUNCT: &[char] = &[',', ';', ':', '!', '?', '"', '\'', '(', ')', '.'];

impl Token {
    /// True only for an unclaimed token — extractors must never match twice on
    /// the same word.
    pub fn is(&self, word: &str) -> bool {
        !self.consumed && self.text == word
    }

    pub fn is_any(&self, words: &[&str]) -> bool {
        !self.consumed && words.contains(&self.text.as_str())
    }
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;

    for (i, c) in input.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                tokens.push(make(input, s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        tokens.push(make(input, s, input.len()));
    }
    tokens
}

fn make(input: &str, start: usize, end: usize) -> Token {
    let raw = &input[start..end];
    Token {
        text: raw.trim_matches(EDGE_PUNCT).to_lowercase(),
        raw: raw.to_string(),
        start,
        end,
        consumed: false,
    }
}

/// Marks `range` consumed and returns its byte span in the original input.
pub fn consume(tokens: &mut [Token], range: Range<usize>) -> Range<usize> {
    let span = tokens[range.start].start..tokens[range.end - 1].end;
    for token in &mut tokens[range] {
        token.consumed = true;
    }
    span
}

/// The matching text of the token at `i`, or `None` if there is none or it is
/// already claimed. Multi-token patterns must read through this: reaching across
/// a token another extractor took would match a phrase that is not there.
pub fn word_at(tokens: &[Token], i: usize) -> Option<&str> {
    tokens.get(i).filter(|t| !t.consumed).map(|t| t.text.as_str())
}

/// The text an extractor matched, as it appeared in the input.
pub fn span_text(input: &str, span: &Range<usize>) -> String {
    input[span.clone()].to_string()
}
