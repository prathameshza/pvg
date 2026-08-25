use std::fmt;

/// The category of error encountered during PVG processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PvgErrorKind {
    /// Lexical tokenization error (e.g. invalid character, unclosed string, tab indentation).
    Lex,
    /// Syntax/Grammar parsing error (e.g. unexpected token, missing property).
    Parse,
    /// Runtime evaluation error (e.g. type mismatch, undefined variable or function).
    Runtime,
    /// Execution safety limit exceeded (e.g. loop iteration limit, stack depth limit).
    SafetyLimit,
}

/// An error that occurred while tokenizing, parsing, or evaluating a PVG document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PvgError {
    /// The classification of this error.
    pub kind: PvgErrorKind,
    /// Human-readable error description.
    pub message: String,
    /// 1-based line number where the error occurred, if known.
    pub line: Option<usize>,
    /// 1-based column number where the error occurred, if known.
    pub col: Option<usize>,
}

impl PvgError {
    /// Creates a lexical tokenization error with line and column position.
    pub fn lex(line: usize, col: usize, msg: impl Into<String>) -> Self {
        Self {
            kind: PvgErrorKind::Lex,
            message: msg.into(),
            line: Some(line),
            col: Some(col),
        }
    }

    /// Creates a syntax parser error with line and column position.
    pub fn parse(line: usize, col: usize, msg: impl Into<String>) -> Self {
        Self {
            kind: PvgErrorKind::Parse,
            message: msg.into(),
            line: Some(line),
            col: Some(col),
        }
    }

    /// Creates a syntax parser error with only a line position.
    pub fn parse_line(line: usize, msg: impl Into<String>) -> Self {
        Self {
            kind: PvgErrorKind::Parse,
            message: msg.into(),
            line: Some(line),
            col: None,
        }
    }

    /// Creates a runtime evaluation error.
    pub fn runtime(msg: impl Into<String>) -> Self {
        Self {
            kind: PvgErrorKind::Runtime,
            message: msg.into(),
            line: None,
            col: None,
        }
    }

    /// Creates an execution safety limit error.
    pub fn safety_limit(msg: impl Into<String>) -> Self {
        Self {
            kind: PvgErrorKind::SafetyLimit,
            message: msg.into(),
            line: None,
            col: None,
        }
    }
}

impl fmt::Display for PvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.col) {
            (Some(l), Some(c)) => write!(f, "Line {}, Col {}: {}", l, c, self.message),
            (Some(l), None) => write!(f, "Line {}: {}", l, self.message),
            (None, _) => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for PvgError {}

impl From<PvgError> for String {
    fn from(err: PvgError) -> Self {
        err.to_string()
    }
}

impl From<String> for PvgError {
    fn from(msg: String) -> Self {
        PvgError::runtime(msg)
    }
}

impl From<&str> for PvgError {
    fn from(msg: &str) -> Self {
        PvgError::runtime(msg)
    }
}