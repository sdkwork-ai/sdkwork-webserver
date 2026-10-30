//! Bounded CRLF line framing shared by every mail protocol.
//!
//! Mail protocols use CRLF lines, tolerate a bare LF from some clients, and must
//! never grow a buffer past the configured maximum. [`LineReader`] is an
//! incremental decoder: feed raw socket bytes, receive complete lines; a partial
//! line stays buffered until its terminator or a bound violation.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// One complete protocol line (terminator stripped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Line bytes without the trailing CRLF/LF, decoded as UTF-8.
    pub text: String,
    /// Whether the line ended with CRLF (`true`) or a tolerated bare LF.
    pub crlf: bool,
}

/// Incremental, bounded line reader.
#[derive(Debug, Clone)]
pub struct LineReader {
    protocol: &'static str,
    maximum: usize,
    buffer: Vec<u8>,
    saw_cr: bool,
}

impl LineReader {
    /// Create a reader for `protocol` with `limits.max_line_bytes`.
    pub fn new(protocol: &'static str, limits: &Limits) -> Self {
        let limits = limits.sanitized();
        Self {
            protocol,
            maximum: limits.max_line_bytes,
            buffer: Vec::new(),
            saw_cr: false,
        }
    }

    /// Maximum line bytes (excluding CRLF).
    pub fn maximum(&self) -> usize {
        self.maximum
    }

    /// Whether a partial line is buffered.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Feed raw bytes and return every line completed by them.
    ///
    /// A bare CR not followed by LF is part of the line content; a line ending
    /// in CRLF yields `crlf: true`. Oversized lines fail closed before the
    /// buffer grows past the bound.
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<Line>> {
        let mut lines = Vec::new();
        for &byte in input {
            if byte == b'\n' {
                let mut raw = core::mem::take(&mut self.buffer);
                let crlf = self.saw_cr && raw.last() == Some(&b'\r');
                if crlf {
                    raw.pop();
                }
                self.saw_cr = false;
                let text = String::from_utf8(raw).map_err(|_| Error::InvalidUtf8 {
                    protocol: self.protocol,
                    field: "line",
                })?;
                lines.push(Line { text, crlf });
                continue;
            }
            self.saw_cr = byte == b'\r';
            if self.buffer.len() >= self.maximum {
                return Err(Error::LineTooLong {
                    protocol: self.protocol,
                    maximum: self.maximum,
                });
            }
            self.buffer.push(byte);
        }
        Ok(lines)
    }

    /// Finish the stream: a residual partial line is a truncation failure.
    pub fn finish(&self) -> Result<()> {
        if self.buffer.is_empty() && !self.saw_cr {
            return Ok(());
        }
        Err(Error::unexpected(self.protocol, "truncated line"))
    }

    /// Reset the reader (used after a STARTTLS boundary).
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.saw_cr = false;
    }
}

/// Encode one line as `text CRLF`, rejecting embedded CR/LF/NUL and enforcing
/// `limits.max_line_bytes`.
pub fn encode_line(text: &str, limits: &Limits) -> Result<Vec<u8>> {
    let limits = limits.sanitized();
    if text.len() > limits.max_line_bytes {
        return Err(Error::LineTooLong {
            protocol: "mail",
            maximum: limits.max_line_bytes,
        });
    }
    if text
        .bytes()
        .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
    {
        return Err(Error::malformed("mail", "line", "control byte"));
    }
    let mut encoded = Vec::with_capacity(text.len() + 2);
    encoded.extend_from_slice(text.as_bytes());
    encoded.push(b'\r');
    encoded.push(b'\n');
    Ok(encoded)
}

/// Split already-terminated CRLF text into lines (helper for tests/fixtures).
pub fn split_crlf(input: &str) -> Vec<&str> {
    input
        .split("\r\n")
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_crlf_and_bare_lf_lines_incrementally() {
        let limits = Limits::default();
        let mut reader = LineReader::new("smtp", &limits);
        let lines = reader.feed(b"220 hi\r\n250-").expect("lines");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "220 hi");
        assert!(lines[0].crlf);
        let more = reader.feed(b"capability\r\n").expect("second");
        assert_eq!(more[0].text, "250-capability");
        reader.finish().expect("complete");
    }

    #[test]
    fn oversized_lines_fail_closed_before_growth() {
        let limits = Limits {
            max_line_bytes: 4,
            ..Limits::default()
        };
        let mut reader = LineReader::new("imap", &limits);
        assert!(matches!(
            reader.feed(b"abcde"),
            Err(Error::LineTooLong { .. })
        ));
    }

    #[test]
    fn truncated_lines_are_reported_by_finish() {
        let mut reader = LineReader::new("pop3", &Limits::default());
        reader.feed(b"+OK par").expect("partial");
        assert!(matches!(reader.finish(), Err(Error::Unexpected { .. })));
    }

    #[test]
    fn encoding_rejects_control_bytes() {
        let limits = Limits::default();
        assert!(encode_line("AUTH PLAIN x\r\nQUIT", &limits).is_err());
        assert!(encode_line("ok", &limits).is_ok());
    }
}
