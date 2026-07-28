use std::fmt;
use std::io;

/// Error returned while parsing or encoding JSON.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    line: usize,
    column: usize,
}

#[derive(Debug)]
enum ErrorKind {
    Message(String),
    Io(io::Error),
}

impl Error {
    pub(crate) fn syntax(message: impl Into<String>, input: &[u8], index: usize) -> Self {
        let end = index.min(input.len());
        let line = 1 + memchr::memchr_iter(b'\n', &input[..end]).count();
        let column = input[..end]
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(end + 1, |position| end - position);
        Self {
            kind: ErrorKind::Message(message.into()),
            line,
            column,
        }
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Message(message.into()),
            line: 0,
            column: 0,
        }
    }

    /// One-based line of a syntax error, or zero for a non-syntax error.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// One-based column of a syntax error, or zero for a non-syntax error.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// Returns true when the error originated from an output writer.
    #[must_use]
    pub const fn is_io(&self) -> bool {
        matches!(self.kind, ErrorKind::Io(_))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Message(message) if self.line != 0 => {
                write!(
                    formatter,
                    "{message} at line {} column {}",
                    self.line, self.column
                )
            }
            ErrorKind::Message(message) => formatter.write_str(message),
            ErrorKind::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Io(error) => Some(error),
            ErrorKind::Message(_) => None,
        }
    }
}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::message(message.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::message(message.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io(error),
            line: 0,
            column: 0,
        }
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        match error.kind {
            ErrorKind::Io(error) => error,
            ErrorKind::Message(message) => Self::new(io::ErrorKind::InvalidData, message),
        }
    }
}

/// Result alias used by this crate.
pub type Result<T> = std::result::Result<T, Error>;
