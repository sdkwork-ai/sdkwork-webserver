//! nginx-compatible `XCLIENT` command builder and parser.
//!
//! nginx's mail proxy sends `XCLIENT ADDR=[IPV6:]addr [LOGIN=login] NAME=host`
//! (with `NGX_ESCAPE_MAIL_XTEXT` escaping on the login) to a Postfix-style
//! backend after `EHLO`. This module builds that bounded line and parses it
//! back for tests and backends.

use core::net::IpAddr;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// One `XCLIENT` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XclientAttribute {
    /// `ADDR=<ip>` (IPv6 prefixed with `IPV6:`).
    Addr(IpAddr),
    /// `LOGIN=<xtext-escaped login>`.
    Login(String),
    /// `NAME=<client hostname>`.
    Name(String),
    /// `HELO=<HELO/EHLO name>`.
    Helo(String),
    /// `PROTO=<SMTP|ESMTP>`.
    Proto(String),
    /// `DESTADDR=<ip>`.
    DestAddr(IpAddr),
    /// `PORT=<destination port>`.
    Port(u16),
}

impl XclientAttribute {
    fn key(&self) -> &'static str {
        match self {
            Self::Addr(_) => "ADDR",
            Self::Login(_) => "LOGIN",
            Self::Name(_) => "NAME",
            Self::Helo(_) => "HELO",
            Self::Proto(_) => "PROTO",
            Self::DestAddr(_) => "DESTADDR",
            Self::Port(_) => "PORT",
        }
    }

    fn encoded_value(&self) -> String {
        match self {
            Self::Addr(ip) | Self::DestAddr(ip) => match ip {
                IpAddr::V4(_) => ip.to_string(),
                IpAddr::V6(_) => format!("IPV6:{ip}"),
            },
            Self::Login(login) => xtext_escape(login),
            Self::Name(name) | Self::Helo(name) | Self::Proto(name) => name.clone(),
            Self::Port(port) => port.to_string(),
        }
    }

    fn parse(key: &str, value: &str) -> Result<Self> {
        match key {
            "ADDR" | "DESTADDR" => {
                let value = value.strip_prefix("IPV6:").unwrap_or(value);
                let ip = value.parse::<IpAddr>().map_err(|_| Error::Xclient {
                    reason: "invalid address",
                })?;
                Ok(if key == "ADDR" {
                    Self::Addr(ip)
                } else {
                    Self::DestAddr(ip)
                })
            }
            "LOGIN" => Ok(Self::Login(xtext_unescape(value)?)),
            "NAME" => {
                validate_name(value)?;
                Ok(Self::Name(value.to_owned()))
            }
            "HELO" => {
                validate_name(value)?;
                Ok(Self::Helo(value.to_owned()))
            }
            "PROTO" => {
                if !value.eq_ignore_ascii_case("SMTP") && !value.eq_ignore_ascii_case("ESMTP") {
                    return Err(Error::Xclient {
                        reason: "invalid PROTO",
                    });
                }
                Ok(Self::Proto(value.to_ascii_uppercase()))
            }
            "PORT" => value
                .parse::<u16>()
                .map(Self::Port)
                .map_err(|_| Error::Xclient {
                    reason: "invalid port",
                }),
            _ => Err(Error::Xclient {
                reason: "unknown attribute",
            }),
        }
    }
}

/// One bounded `XCLIENT` command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XclientRequest {
    attributes: Vec<XclientAttribute>,
}

impl XclientRequest {
    /// Empty request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an attribute (later calls keep insertion order).
    ///
    /// # Errors
    ///
    /// Fails closed when the attribute count exceeds
    /// `limits.max_xclient_attributes`, or for an invalid name/login value.
    pub fn push(&mut self, attribute: XclientAttribute, limits: &Limits) -> Result<()> {
        validate_attribute(&attribute)?;
        if self.attributes.len() >= limits.sanitized().max_xclient_attributes {
            return Err(Error::limit(
                "xclient",
                "attributes",
                limits.sanitized().max_xclient_attributes,
            ));
        }
        self.attributes.push(attribute);
        Ok(())
    }

    /// Attributes in insertion order.
    pub fn attributes(&self) -> &[XclientAttribute] {
        &self.attributes
    }

    /// Encode `XCLIENT k=v k=v` (without CRLF).
    ///
    /// # Errors
    ///
    /// Fails closed for an invalid attribute or an oversized line.
    pub fn encode_line(&self, limits: &Limits) -> Result<String> {
        let limits = limits.sanitized();
        let mut line = String::from("XCLIENT");
        for attribute in &self.attributes {
            validate_attribute(attribute)?;
            line.push(' ');
            line.push_str(attribute.key());
            line.push('=');
            line.push_str(&attribute.encoded_value());
        }
        if line.len() > limits.max_relay_header_bytes {
            return Err(Error::limit(
                "xclient",
                "header bytes",
                limits.max_relay_header_bytes,
            ));
        }
        Ok(line)
    }

    /// Parse an `XCLIENT ...` line.
    ///
    /// # Errors
    ///
    /// Fails closed on unknown attributes, invalid values, or oversized lines.
    pub fn parse(line: &str, limits: &Limits) -> Result<Self> {
        let limits = limits.sanitized();
        if line.len() > limits.max_relay_header_bytes {
            return Err(Error::limit(
                "xclient",
                "header bytes",
                limits.max_relay_header_bytes,
            ));
        }
        let mut parts = line.split_whitespace();
        if parts.next() != Some("XCLIENT") {
            return Err(Error::Xclient {
                reason: "missing XCLIENT verb",
            });
        }
        let mut request = Self::new();
        for part in parts {
            let (key, value) = part.split_once('=').ok_or(Error::Xclient {
                reason: "missing =",
            })?;
            request.push(XclientAttribute::parse(key, value)?, &limits)?;
        }
        Ok(request)
    }
}

fn validate_attribute(attribute: &XclientAttribute) -> Result<()> {
    match attribute {
        XclientAttribute::Name(name) | XclientAttribute::Helo(name) => validate_name(name),
        XclientAttribute::Login(login) => {
            if login.is_empty() || login.len() > 255 {
                return Err(Error::Xclient {
                    reason: "invalid login",
                });
            }
            Ok(())
        }
        XclientAttribute::Proto(proto) => {
            if !proto.eq_ignore_ascii_case("SMTP") && !proto.eq_ignore_ascii_case("ESMTP") {
                return Err(Error::Xclient {
                    reason: "invalid PROTO",
                });
            }
            Ok(())
        }
        XclientAttribute::Addr(_) | XclientAttribute::DestAddr(_) | XclientAttribute::Port(_) => {
            Ok(())
        }
    }
}

fn validate_name(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 255 {
        return Err(Error::Xclient {
            reason: "invalid name",
        });
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'-' | b'_' | b'[' | b']' | b':' | b'/' | b'?')
    }) {
        return Err(Error::Xclient {
            reason: "invalid name character",
        });
    }
    Ok(())
}

fn xtext_escape(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        let needs_escape = byte <= 0x20 || byte >= 0x7f || matches!(byte, b'+' | b'=' | b'%');
        if needs_escape {
            escaped.push('%');
            escaped.push(HEX[(byte >> 4) as usize] as char);
            escaped.push(HEX[(byte & 0x0f) as usize] as char);
        } else {
            escaped.push(byte as char);
        }
    }
    escaped
}

fn xtext_unescape(value: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut iter = value.bytes();
    while let Some(byte) = iter.next() {
        if byte == b'%' {
            let high = iter.next().ok_or(Error::Xclient {
                reason: "truncated escape",
            })?;
            let low = iter.next().ok_or(Error::Xclient {
                reason: "truncated escape",
            })?;
            let decode = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            let high = decode(high).ok_or(Error::Xclient {
                reason: "invalid escape",
            })?;
            let low = decode(low).ok_or(Error::Xclient {
                reason: "invalid escape",
            })?;
            bytes.push((high << 4) | low);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).map_err(|_| Error::Xclient {
        reason: "invalid UTF-8",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproduces_the_nginx_xclient_line() {
        let limits = Limits::default();
        let mut request = XclientRequest::new();
        request
            .push(
                XclientAttribute::Addr("203.0.113.5".parse().expect("ip")),
                &limits,
            )
            .expect("addr");
        request
            .push(XclientAttribute::Login("alice+tag".to_owned()), &limits)
            .expect("login");
        request
            .push(
                XclientAttribute::Name("mail.example.com".to_owned()),
                &limits,
            )
            .expect("name");
        let line = request.encode_line(&limits).expect("line");
        assert_eq!(
            line,
            "XCLIENT ADDR=203.0.113.5 LOGIN=alice%2Btag NAME=mail.example.com"
        );
        let parsed = XclientRequest::parse(&line, &limits).expect("parse");
        assert_eq!(parsed, request);
    }

    #[test]
    fn ipv6_addresses_use_the_ipv6_prefix() {
        let limits = Limits::default();
        let mut request = XclientRequest::new();
        request
            .push(
                XclientAttribute::Addr("2001:db8::1".parse().expect("ip")),
                &limits,
            )
            .expect("addr");
        let line = request.encode_line(&limits).expect("line");
        assert_eq!(line, "XCLIENT ADDR=IPV6:2001:db8::1");
        assert_eq!(
            XclientRequest::parse(&line, &limits).expect("parse"),
            request
        );
    }

    #[test]
    fn invalid_attributes_and_oversized_lines_fail_closed() {
        let limits = Limits {
            max_xclient_attributes: 1,
            max_relay_header_bytes: 32,
            ..Limits::default()
        };
        let mut request = XclientRequest::new();
        request
            .push(
                XclientAttribute::Addr("203.0.113.5".parse().expect("ip")),
                &limits,
            )
            .expect("addr");
        assert!(request
            .push(XclientAttribute::Login("alice".to_owned()), &limits)
            .is_err());
        assert!(XclientRequest::parse("XCLIENT BOGUS=1", &Limits::default()).is_err());
        assert!(XclientRequest::parse("XCLIENT ADDR=not-an-ip", &Limits::default()).is_err());
        assert!(XclientRequest::parse("", &Limits::default()).is_err());
    }
}
