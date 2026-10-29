//! Span-carrying parse errors and "did you mean" suggestions.
//!
//! Parsers deep in the tree only know byte spans; `config::error` turns a [`Diag`]
//! into a user-facing error with file, line, column and a source excerpt.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Diag {
    pub span: Range<usize>,
    pub message: String,
    pub help: Option<String>,
}

impl Diag {
    pub fn new(span: Range<usize>, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            help: None,
        }
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Adds a did-you-mean help line when a close candidate exists.
    pub fn suggest<'a>(self, input: &str, candidates: impl IntoIterator<Item = &'a str>) -> Self {
        match did_you_mean(input, candidates) {
            Some(s) => self.help(format!("did you mean \"{s}\"?")),
            None => self,
        }
    }
}

/// Closest candidate by edit distance, if it's close enough to be a plausible typo.
pub(crate) fn did_you_mean<'a>(
    input: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = 2.max(input.chars().count() / 3);
    candidates
        .into_iter()
        .map(|c| (strsim::damerau_levenshtein(input, c), c))
        .filter(|&(d, _)| d <= limit)
        .min_by_key(|&(d, _)| d)
        .map(|(_, c)| c)
}

/// 1-based line and (char) column of a byte offset.
pub(crate) fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = &src[..floor_char_boundary(src, offset)];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count() + 1)
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_matches_only() {
        let ids = ["split-pane", "close-pane", "new-tab"];
        assert_eq!(did_you_mean("split-pain", ids), Some("split-pane"));
        assert_eq!(did_you_mean("new-tba", ids), Some("new-tab"));
        assert_eq!(did_you_mean("frobnicate", ids), None);
    }

    #[test]
    fn line_col_is_one_based() {
        let src = "a = 1\nbb = 2\n";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 6), (2, 1));
        assert_eq!(line_col(src, 9), (2, 4));
    }
}
