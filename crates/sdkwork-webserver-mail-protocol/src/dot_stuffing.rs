//! Dot-stuffed multiline framing (POP3 `+OK`/`CAPA` bodies, SMTP `DATA`).
//!
//! Multiline payloads use CRLF lines and a lone `.` line as the terminator; a
//! payload line that starts with `.` is emitted with an extra leading dot and
//! unstuffed on receipt. The decoder is incremental and bounded by line count
//! and total bytes.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Terminator line (without CRLF).
pub const TERMINATOR: &str = ".";

/// Encode payload lines into one dot-stuffed, terminated block.
///
/// Each line is dot-stuffed when it starts with `.`, terminated with CRLF, and
/// the block ends with `.` CRLF.
///
/// # Errors
///
/// Fails closed when a line contains CR/LF/NUL, the line count exceeds
/// `limits.max_multiline_lines`, or the encoded block exceeds
/// `limits.max_multiline_bytes`.
pub fn encode_multiline<'a, I>(lines: I, limits: &Limits) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = &'a str>,
{
    let limits = limits.sanitized();
    let mut encoded = Vec::new();
    let mut count = 0usize;
    for line in lines {
        count += 1;
        if count > limits.max_multiline_lines {
            return Err(Error::limit(
                "mail",
                "multiline lines",
                limits.max_multiline_lines,
            ));
        }
        if line
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
        {
            return Err(Error::malformed("mail", "multiline line", "control byte"));
        }
        if line.len() > limits.max_line_bytes {
            return Err(Error::LineTooLong {
                protocol: "mail",
                maximum: limits.max_line_bytes,
            });
        }
        if line.starts_with('.') {
            encoded.push(b'.');
        }
        encoded.extend_from_slice(line.as_bytes());
        encoded.extend_from_slice(b"\r\n");
        if encoded.len() > limits.max_multiline_bytes {
            return Err(Error::limit(
                "mail",
                "multiline bytes",
                limits.max_multiline_bytes,
            ));
        }
    }
    encoded.extend_from_slice(b".\r\n");
    Ok(encoded)
}

/// Incremental dot-stuffing decoder for one multiline response.
#[derive(Debug, Clone)]
pub struct DotStuffingReader {
    protocol: &'static str,
    limits: Limits,
    current: Vec<u8>,
    lines: Vec<String>,
    encoded_bytes: usize,
    complete: bool,
}

impl DotStuffingReader {
    /// Create a decoder for `protocol` with the supplied bounds.
    pub fn new(protocol: &'static str, limits: &Limits) -> Self {
        Self {
            protocol,
            limits: limits.sanitized(),
            current: Vec::new(),
            lines: Vec::new(),
            encoded_bytes: 0,
            complete: false,
        }
    }

    /// Feed raw bytes; returns newly completed payload lines. The terminator is
    /// consumed and marks the reader complete.
    ///
    /// # Errors
    ///
    /// Fails closed on oversized lines/byte counts, invalid UTF-8, or bytes
    /// after the terminator.
    pub fn feed(&mut self, input: &[u8]) -> Result<Vec<String>> {
        let mut decoded = Vec::new();
        for &byte in input {
            if self.complete {
                return Err(Error::TrailingBytes {
                    protocol: self.protocol,
                });
            }
            self.encoded_bytes += 1;
            if self.encoded_bytes > self.limits.max_multiline_bytes {
                return Err(Error::limit(
                    self.protocol,
                    "multiline bytes",
                    self.limits.max_multiline_bytes,
                ));
            }
            if byte == b'\n' {
                let mut raw = core::mem::take(&mut self.current);
                if raw.last() == Some(&b'\r') {
                    raw.pop();
                }
                if raw == b"." {
                    self.complete = true;
                    continue;
                }
                if raw.first() == Some(&b'.') {
                    raw.remove(0);
                }
                if raw.len() > self.limits.max_line_bytes {
                    return Err(Error::LineTooLong {
                        protocol: self.protocol,
                        maximum: self.limits.max_line_bytes,
                    });
                }
                let text = String::from_utf8(raw).map_err(|_| Error::InvalidUtf8 {
                    protocol: self.protocol,
                    field: "multiline line",
                })?;
                if self.lines.len() + 1 > self.limits.max_multiline_lines {
                    return Err(Error::limit(
                        self.protocol,
                        "multiline lines",
                        self.limits.max_multiline_lines,
                    ));
                }
                self.lines.push(text.clone());
                decoded.push(text);
                continue;
            }
            if self.current.len() >= self.limits.max_line_bytes {
                return Err(Error::LineTooLong {
                    protocol: self.protocol,
                    maximum: self.limits.max_line_bytes,
                });
            }
            self.current.push(byte);
        }
        Ok(decoded)
    }

    /// Whether the `.` terminator was seen.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// All payload lines decoded so far.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Validate that the response was terminated.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnterminatedMultiline`] when the terminator is missing.
    pub fn finish(&self) -> Result<()> {
        if self.complete {
            Ok(())
        } else {
            Err(Error::UnterminatedMultiline {
                protocol: self.protocol,
            })
        }
    }
}

/// Decode one complete dot-stuffed block (helper for tests and fixtures).
///
/// # Errors
///
/// Fails closed on malformed input, trailing bytes, or a missing terminator.
pub fn decode_multiline(
    protocol: &'static str,
    input: &[u8],
    limits: &Limits,
) -> Result<Vec<String>> {
    let mut reader = DotStuffingReader::new(protocol, limits);
    let lines = reader.feed(input)?;
    reader.finish()?;
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_dot_stuffed_payloads() {
        let limits = Limits::default();
        let encoded =
            encode_multiline(["line one", ".starts-with-dot", "..two"], &limits).expect("encode");
        assert_eq!(encoded, b"line one\r\n..starts-with-dot\r\n...two\r\n.\r\n");
        let decoded = decode_multiline("smtp", &encoded, &limits).expect("decode");
        assert_eq!(decoded, ["line one", ".starts-with-dot", "..two"]);
    }

    #[test]
    fn empty_payload_is_just_the_terminator() {
        let limits = Limits::default();
        let encoded = encode_multiline(std::iter::empty::<&str>(), &limits).expect("encode");
        assert_eq!(encoded, b".\r\n");
        assert!(decode_multiline("pop3", &encoded, &limits)
            .expect("decode")
            .is_empty());
    }

    #[test]
    fn unterminated_and_trailing_bytes_fail_closed() {
        let limits = Limits::default();
        assert!(matches!(
            decode_multiline("pop3", b"line\r\n", &limits),
            Err(Error::UnterminatedMultiline { .. })
        ));
        assert!(matches!(
            decode_multiline("pop3", b"line\r\n.\r\nextra", &limits),
            Err(Error::TrailingBytes { .. })
        ));
    }

    #[test]
    fn limits_are_enforced_before_growth() {
        let limits = Limits {
            max_multiline_lines: 2,
            ..Limits::default()
        };
        assert!(encode_multiline(["a", "b", "c"], &limits).is_err());
        let mut reader = DotStuffingReader::new("smtp", &limits);
        reader.feed(b"a\r\nb\r\n").expect("two lines");
        assert!(matches!(
            reader.feed(b"c\r\n"),
            Err(Error::LimitExceeded { .. })
        ));
    }

    #[test]
    fn adversarial_bytes_never_panic() {
        let limits = Limits::default();
        for input in [
            b"\r\n.\r\n".as_slice(),
            b"..\r\n.\r\n",
            b".\r\n.\r\n",
            b"no-newline",
            b"\xff\xfe\r\n.\r\n",
        ] {
            let _ = decode_multiline("smtp", input, &limits);
        }
    }
}
