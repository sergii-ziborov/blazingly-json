/// A zero-allocation recognizer for a known canonical JSON layout.
///
/// This is not a general JSON parser. It is an optimization primitive for
/// generated or protocol-specific code that first attempts one exact layout
/// and falls back to [`crate::Cursor`] when any expectation does not match.
/// Every successful recognizer must consume and check the complete input.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalScanner<'a> {
    remaining: &'a [u8],
}

impl<'a> CanonicalScanner<'a> {
    /// Starts matching an input slice.
    #[must_use]
    #[inline]
    pub const fn new(input: &'a [u8]) -> Self {
        Self { remaining: input }
    }

    /// Consumes an exact byte sequence.
    ///
    /// Returns `None` without advancing when the sequence does not match.
    #[inline]
    pub fn literal(&mut self, expected: &[u8]) -> Option<()> {
        self.remaining = self.remaining.strip_prefix(expected)?;
        Some(())
    }

    /// Consumes a JSON string with no escapes and returns its borrowed value.
    ///
    /// Escaped strings deliberately return `None` so the caller can use the
    /// general parser instead.
    #[inline]
    pub fn plain_string(&mut self) -> Option<&'a str> {
        let input = self.remaining.strip_prefix(b"\"")?;
        let end = memchr::memchr(b'"', input)?;
        let value = &input[..end];
        if value.iter().any(|byte| *byte == b'\\' || *byte < 0x20) {
            return None;
        }
        let value = std::str::from_utf8(value).ok()?;
        self.remaining = &input[end + 1..];
        Some(value)
    }

    /// Consumes one unsigned JSON integer.
    #[inline]
    pub fn unsigned(&mut self) -> Option<u64> {
        let first = *self.remaining.first()?;
        if !first.is_ascii_digit() {
            return None;
        }
        if first == b'0' && self.remaining.get(1).is_some_and(u8::is_ascii_digit) {
            return None;
        }

        let mut value = 0_u64;
        let mut length = 0;
        for &digit in self.remaining {
            if !digit.is_ascii_digit() {
                break;
            }
            value = value
                .checked_mul(10)?
                .checked_add(u64::from(digit - b'0'))?;
            length += 1;
        }
        self.remaining = &self.remaining[length..];
        Some(value)
    }

    /// Consumes `true` or `false`.
    #[inline]
    pub fn boolean(&mut self) -> Option<bool> {
        if let Some(remaining) = self.remaining.strip_prefix(b"true") {
            self.remaining = remaining;
            Some(true)
        } else if let Some(remaining) = self.remaining.strip_prefix(b"false") {
            self.remaining = remaining;
            Some(false)
        } else {
            None
        }
    }

    /// Returns the unmatched suffix.
    #[must_use]
    #[inline]
    pub const fn remaining(&self) -> &'a [u8] {
        self.remaining
    }

    /// Returns true only when the recognizer consumed the complete input.
    #[must_use]
    #[inline]
    pub const fn is_finished(&self) -> bool {
        self.remaining.is_empty()
    }
}
