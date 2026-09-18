//! Whitespace tokenizer that keeps byte spans, so every extractor can report
//! exactly which slice of the input produced a field — and so the title can be
//! rebuilt from whatever nobody claimed.

use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Token {
    /// Lowercased, punctuation-trimmed form. Used for all matching.
    pub text: String,
    /// The original slice, used to rebuild the title with its casing intact.
    pub raw: String,
    pub start: usize,
    pub end: usize,
    /// Set once an extractor claims this token.
    pub consumed: bool,
    /// The user reverted this span back to plain text, so no extractor may
    /// claim it and it stays in the title.
    pub excluded: bool,
}

/// Punctuation trimmed from the end of a token: "5pm," and "gym!" should match
/// as "5pm" and "gym".
const TRAILING_PUNCT: &[char] = &[',', ';', ':', '!', '?', '"', '\'', ')', '.'];

/// Punctuation trimmed from the start. `!` is absent deliberately — it is the
/// priority sigil, so `!p1` must survive with its marker intact.
const LEADING_PUNCT: &[char] = &[',', ';', ':', '?', '"', '\'', '(', '.'];

impl Token {
    /// Whether an extractor may look at this token at all: not already claimed,
    /// and not one the user reverted to plain text.
    pub fn available(&self) -> bool {
        !self.consumed && !self.excluded
    }

    /// True only for an available token — extractors must never match twice on
    /// the same word, nor re-claim something the user has un-parsed.
    pub fn is(&self, word: &str) -> bool {
        self.available() && self.text == word
    }

    pub fn is_any(&self, words: &[&str]) -> bool {
        self.available() && words.contains(&self.text.as_str())
    }
}

/// Tokenizes, marking any token that overlaps one of `excluded` as off limits.
///
/// This is how "un-parsing" works: the client remembers which byte ranges the
/// user reverted and hands them back, and those words stay ordinary text no
/// matter how much they look like a date.
pub fn tokenize_excluding(input: &str, excluded: &[Range<usize>]) -> Vec<Token> {
    let mut tokens = tokenize_all(input);
    for token in &mut tokens {
        token.excluded = excluded.iter().any(|range| overlaps(token.start..token.end, range));
    }
    tokens
}

fn overlaps(a: Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn tokenize_all(input: &str) -> Vec<Token> {
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

    // A token that is nothing but punctuation keeps it — "!!!" is a priority,
    // not an empty token.
    let trimmed = raw.trim_end_matches(TRAILING_PUNCT);
    let trimmed = if trimmed.is_empty() { raw } else { trimmed };
    let text = trimmed.trim_start_matches(LEADING_PUNCT);
    let text = if text.is_empty() { trimmed } else { text };

    Token {
        text: text.to_lowercase(),
        raw: raw.to_string(),
        start,
        end,
        consumed: false,
        excluded: false,
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
    tokens.get(i).filter(|t| t.available()).map(|t| t.text.as_str())
}

/// The text an extractor matched, as it appeared in the input.
pub fn span_text(input: &str, span: &Range<usize>) -> String {
    input[span.clone()].to_string()
}
