//! Incremental HTTP-like response stream parser shared by the protocols whose
//! response is one header block followed by raw body bytes (uWSGI, SCGI).
//!
//! FastCGI does not use this module directly because its response is framed in
//! records; it reuses [`crate::header_block`] instead.

use crate::{
    error::{Error, Result},
    header_block::{parse_response_head, HeaderStyle, HttpResponseHead},
    limits::Limits,
};

/// A response-stream event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamEvent {
    /// The header block was parsed. Emitted exactly once.
    Head(HttpResponseHead),
    /// Raw body bytes following the header block.
    Body(Vec<u8>),
}

/// Incremental `header block + body` parser.
#[derive(Debug, Clone)]
pub(crate) struct ResponseStreamParser {
    protocol: &'static str,
    style: HeaderStyle,
    limits: Limits,
    buffer: Vec<u8>,
    head: Option<HttpResponseHead>,
}

impl ResponseStreamParser {
    pub(crate) fn new(protocol: &'static str, style: HeaderStyle, limits: Limits) -> Self {
        Self {
            protocol,
            style,
            limits: limits.sanitized(),
            buffer: Vec::new(),
            head: None,
        }
    }

    pub(crate) fn feed(&mut self, input: &[u8]) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();
        if self.head.is_none() {
            self.buffer.extend_from_slice(input);
            let Some((head, consumed)) =
                parse_response_head(self.protocol, &self.buffer, self.style, &self.limits)?
            else {
                return Ok(events);
            };
            self.head = Some(head.clone());
            events.push(StreamEvent::Head(head));
            let remainder = self.buffer.split_off(consumed.min(self.buffer.len()));
            if !remainder.is_empty() {
                events.push(StreamEvent::Body(remainder));
            }
            return Ok(events);
        }
        if !input.is_empty() {
            events.push(StreamEvent::Body(input.to_vec()));
        }
        Ok(events)
    }

    pub(crate) fn head(&self) -> Option<&HttpResponseHead> {
        self.head.as_ref()
    }

    pub(crate) fn finish(&self) -> Result<()> {
        if self.head.is_none() {
            return Err(Error::protocol(
                self.protocol,
                "response stream ended before the header block completed",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_and_body_are_split_across_feeds() {
        let mut parser =
            ResponseStreamParser::new("test", HeaderStyle::HttpOrCgi, Limits::default());
        assert!(parser.feed(b"HTTP/1.1 200 OK\r\n").unwrap().is_empty());
        let events = parser
            .feed(b"Content-Length: 3\r\n\r\nabc")
            .expect("events");
        assert!(matches!(events[0], StreamEvent::Head(_)));
        assert_eq!(events[1], StreamEvent::Body(b"abc".to_vec()));
        let events = parser.feed(b"def").expect("body");
        assert_eq!(events, vec![StreamEvent::Body(b"def".to_vec())]);
        parser.finish().expect("complete");
    }

    #[test]
    fn missing_header_block_fails_closed() {
        let mut parser = ResponseStreamParser::new("test", HeaderStyle::Http, Limits::default());
        assert!(parser.feed(b"HTTP/1.1 200").unwrap().is_empty());
        assert!(parser.finish().is_err());
    }
}
