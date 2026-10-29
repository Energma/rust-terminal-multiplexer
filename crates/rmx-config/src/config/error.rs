//! User-facing config errors and warnings: `file:line:col` location, help
//! line and a caret-underlined source excerpt.

use std::fmt;
use std::ops::Range;

use crate::diag::{Diag, line_col};

/// The offending source line plus a caret run under the span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Snippet {
    text: String,
    caret_col: usize,
    caret_len: usize,
}

fn locate(src: &str, span: &Range<usize>) -> (usize, usize, Option<Box<Snippet>>) {
    let (line, col) = line_col(src, span.start);
    let text = src.lines().nth(line - 1).unwrap_or("").to_string();
    let line_len = text.chars().count();
    let span_len = src
        .get(span.clone())
        .map_or(1, |s| s.lines().next().unwrap_or("").chars().count());
    let caret_len = span_len.clamp(1, line_len.saturating_sub(col - 1).max(1));
    let snippet = Snippet {
        text,
        caret_col: col,
        caret_len,
    };
    (line, col, Some(Box::new(snippet)))
}

fn render(
    f: &mut fmt::Formatter<'_>,
    origin: &str,
    line: usize,
    col: usize,
    message: &str,
    help: Option<&str>,
    snippet: Option<&Snippet>,
) -> fmt::Result {
    if line == 0 {
        write!(f, "{origin}: {message}")?;
    } else {
        write!(f, "{origin}:{line}:{col}: {message}")?;
    }
    if let Some(h) = help {
        write!(f, "\n  help: {h}")?;
    }
    if let Some(s) = snippet.filter(|_| line > 0) {
        let gutter = line.to_string().len();
        let pad = " ".repeat(gutter);
        write!(
            f,
            "\n{pad} |\n{line} | {}\n{pad} | {}{}",
            s.text,
            " ".repeat(s.caret_col - 1),
            "^".repeat(s.caret_len)
        )?;
    }
    Ok(())
}

/// A config problem that stops loading. Display gives `file:line:col: message`,
/// an optional help line, then the source line with a caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub origin: String,
    /// 1-based; 0 when the error has no location (e.g. unreadable file).
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub help: Option<String>,
    pub(super) snippet: Option<Box<Snippet>>,
}

impl ConfigError {
    pub(super) fn from_diag(src: &str, origin: &str, d: Diag) -> Self {
        let (line, col, snippet) = locate(src, &d.span);
        Self {
            origin: origin.into(),
            line,
            col,
            message: d.message,
            help: d.help,
            snippet,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        render(
            f,
            &self.origin,
            self.line,
            self.col,
            &self.message,
            self.help.as_deref(),
            self.snippet.as_deref(),
        )
    }
}

impl std::error::Error for ConfigError {}

/// A config problem that doesn't stop loading (the binding still applies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub origin: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub help: Option<String>,
    snippet: Option<Box<Snippet>>,
}

impl Warning {
    pub(super) fn from_diag(src: &str, origin: &str, d: Diag) -> Self {
        let (line, col, snippet) = locate(src, &d.span);
        Self {
            origin: origin.into(),
            line,
            col,
            message: d.message,
            help: d.help,
            snippet,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        render(
            f,
            &self.origin,
            self.line,
            self.col,
            &format!("warning: {}", self.message),
            self.help.as_deref(),
            self.snippet.as_deref(),
        )
    }
}
