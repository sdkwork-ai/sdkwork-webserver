//! IMAP client-side dialog state machine (nginx mail proxy → backend).
//!
//! Owns tagged command builders, greeting/capability parsing, literal-size
//! continuation handling, `AUTHENTICATE PLAIN/LOGIN`, `STARTTLS` boundaries and
//! `BYE` detection. No I/O, no unbounded allocation.

use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::line::encode_line;

const PROTOCOL: &str = "imap";

/// IMAP dialog state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapState {
    /// Waiting for the untagged `* OK`/`* PREAUTH` greeting.
    AwaitGreeting,
    /// Greeting accepted; not authenticated.
    NotAuthenticated,
    /// `LOGIN`/`AUTHENTICATE` succeeded.
    Authenticated,
    /// `STARTTLS` accepted; the caller must run the TLS handshake.
    TlsRequired,
    /// The backend sent `* BYE` or the session finished.
    Closed,
}

/// One tagged IMAP command kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapCommandKind {
    /// `CAPABILITY`
    Capability,
    /// `STARTTLS`
    StartTls,
    /// `LOGIN`
    Login,
    /// `AUTHENTICATE PLAIN`
    AuthenticatePlain,
    /// `AUTHENTICATE LOGIN`
    AuthenticateLogin,
    /// A base64 SASL continuation response.
    AuthContinue,
    /// `LOGOUT`
    Logout,
}

/// Parsed IMAP response event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapEvent {
    /// Untagged `* OK`/`* PREAUTH` greeting.
    Greeting {
        /// Whether the server pre-authenticated the session (`* PREAUTH`).
        preauthenticated: bool,
    },
    /// Untagged `* CAPABILITY` payload.
    Capabilities(Vec<String>),
    /// Server continuation (`+ ...`), used by `AUTHENTICATE LOGIN`.
    AuthContinue(String),
    /// Tagged completion: status `OK`/`NO`/`BAD` plus human text.
    Tagged {
        /// Command tag echoed by the server.
        tag: String,
        /// Completion status.
        status: ImapStatus,
        /// Response text.
        text: String,
    },
    /// Untagged `* BYE`.
    Bye,
    /// A literal of `size` bytes must be supplied before parsing continues.
    LiteralRequired {
        /// Declared literal size.
        size: usize,
    },
    /// Any other untagged response retained verbatim (bounded).
    Untagged(String),
}

/// Tagged completion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapStatus {
    /// `OK`
    Ok,
    /// `NO`
    No,
    /// `BAD`
    Bad,
}

impl ImapStatus {
    fn parse(token: &str) -> Result<Self> {
        match token.to_ascii_uppercase().as_str() {
            "OK" => Ok(Self::Ok),
            "NO" => Ok(Self::No),
            "BAD" => Ok(Self::Bad),
            _ => Err(Error::malformed(
                PROTOCOL,
                "tagged status",
                "expected OK, NO or BAD",
            )),
        }
    }

    /// Uppercase wire token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::No => "NO",
            Self::Bad => "BAD",
        }
    }
}

#[derive(Debug, Clone)]
struct PendingTagged {
    tag: String,
    kind: ImapCommandKind,
}

#[derive(Debug, Clone)]
struct LiteralState {
    size: usize,
    collected: Vec<u8>,
}

/// Bounded IMAP client dialog.
#[derive(Debug, Clone)]
pub struct ImapClient {
    limits: Limits,
    state: ImapState,
    pending: Option<PendingTagged>,
    literal: Option<LiteralState>,
    tag_counter: u32,
}

impl ImapClient {
    /// Create a client awaiting the greeting.
    pub fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            state: ImapState::AwaitGreeting,
            pending: None,
            literal: None,
            tag_counter: 0,
        }
    }

    /// Current state.
    pub fn state(&self) -> ImapState {
        self.state
    }

    /// Bounds in effect.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Allocate the next tag (`A0001`-style) without mutating dialog state.
    ///
    /// # Errors
    ///
    /// Fails closed when the tag space would exceed `u32::MAX` sessions tags.
    pub fn next_tag(&mut self) -> Result<String> {
        self.tag_counter = self
            .tag_counter
            .checked_add(1)
            .ok_or_else(|| Error::limit(PROTOCOL, "tag", u32::MAX as usize))?;
        Ok(format!("A{:04}", self.tag_counter))
    }

    fn issue(&mut self, tag: String, kind: ImapCommandKind, line: &str) -> Result<Vec<u8>> {
        let allowed = match kind {
            ImapCommandKind::Capability => !matches!(self.state, ImapState::Closed),
            ImapCommandKind::StartTls => self.state == ImapState::NotAuthenticated,
            ImapCommandKind::Login
            | ImapCommandKind::AuthenticatePlain
            | ImapCommandKind::AuthenticateLogin => self.state == ImapState::NotAuthenticated,
            ImapCommandKind::AuthContinue => self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.kind == ImapCommandKind::AuthenticateLogin),
            ImapCommandKind::Logout => !matches!(self.state, ImapState::Closed),
        };
        if !allowed || self.pending.is_some() || self.literal.is_some() {
            return Err(Error::unexpected(PROTOCOL, "command in this state"));
        }
        self.pending = Some(PendingTagged {
            tag: tag.clone(),
            kind,
        });
        encode_line(line, &self.limits)
    }

    /// Build `<tag> CAPABILITY`.
    ///
    /// # Errors
    ///
    /// Fails closed in an invalid state.
    pub fn capability(&mut self, tag: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(
            tag_owned,
            ImapCommandKind::Capability,
            &format!("{tag} CAPABILITY"),
        )
    }

    /// Build `<tag> STARTTLS`.
    ///
    /// # Errors
    ///
    /// Fails closed unless the session is not authenticated yet.
    pub fn starttls(&mut self, tag: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(
            tag_owned,
            ImapCommandKind::StartTls,
            &format!("{tag} STARTTLS"),
        )
    }

    /// Build `<tag> LOGIN <user> <password>` with IMAP quoted strings.
    ///
    /// # Errors
    ///
    /// Fails closed for invalid quoting, control bytes, or wrong state.
    pub fn login(&mut self, tag: &str, user: &str, password: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let user = quote_string(PROTOCOL, user, &self.limits)?;
        let password = quote_string(PROTOCOL, password, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(
            tag_owned,
            ImapCommandKind::Login,
            &format!("{tag} LOGIN {user} {password}"),
        )
    }

    /// Build `<tag> AUTHENTICATE PLAIN` (the response follows the `+` prompt).
    ///
    /// # Errors
    ///
    /// Fails closed unless the session is not authenticated yet.
    pub fn authenticate_plain(&mut self, tag: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(
            tag_owned,
            ImapCommandKind::AuthenticatePlain,
            &format!("{tag} AUTHENTICATE PLAIN"),
        )
    }

    /// Build `<tag> AUTHENTICATE LOGIN`.
    ///
    /// # Errors
    ///
    /// Fails closed unless the session is not authenticated yet.
    pub fn authenticate_login(&mut self, tag: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(
            tag_owned,
            ImapCommandKind::AuthenticateLogin,
            &format!("{tag} AUTHENTICATE LOGIN"),
        )
    }

    /// Build a base64 SASL continuation line.
    ///
    /// # Errors
    ///
    /// Fails closed unless an `AUTHENTICATE LOGIN` continuation is pending.
    pub fn auth_response(&mut self, response: &str) -> Result<Vec<u8>> {
        if response.len() > self.limits.max_auth_line_bytes
            || response
                .bytes()
                .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        {
            return Err(Error::malformed(
                PROTOCOL,
                "auth response",
                "forbidden byte",
            ));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(Error::unexpected(PROTOCOL, "auth response"));
        };
        if pending.kind != ImapCommandKind::AuthenticateLogin {
            return Err(Error::unexpected(PROTOCOL, "auth response"));
        }
        self.pending = Some(PendingTagged {
            tag: pending.tag,
            kind: ImapCommandKind::AuthContinue,
        });
        encode_line(response, &self.limits)
    }

    /// Build `<tag> LOGOUT`.
    ///
    /// # Errors
    ///
    /// Fails closed after the session closed.
    pub fn logout(&mut self, tag: &str) -> Result<Vec<u8>> {
        validate_tag(tag, &self.limits)?;
        let tag_owned = tag.to_owned();
        self.issue(tag_owned, ImapCommandKind::Logout, &format!("{tag} LOGOUT"))
    }

    /// Consume one backend line.
    ///
    /// # Errors
    ///
    /// Fails closed on oversized lines, literal overruns, tag mismatches or
    /// malformed responses.
    pub fn on_line(&mut self, line: &str) -> Result<ImapEvent> {
        check_line(line, &self.limits)?;
        if self.literal.is_some() {
            return Err(Error::unexpected(
                PROTOCOL,
                "line while a literal is pending",
            ));
        }
        if let Some(size) = trailing_literal_size(line)? {
            if size > self.limits.max_literal_bytes {
                return Err(Error::LiteralTooLarge {
                    size,
                    maximum: self.limits.max_literal_bytes,
                });
            }
            self.literal = Some(LiteralState {
                size,
                collected: Vec::new(),
            });
            return Ok(ImapEvent::LiteralRequired { size });
        }
        self.parse_line(line)
    }

    /// Feed literal bytes after [`ImapEvent::LiteralRequired`].
    ///
    /// Returns `Some(event)` once the declared literal is complete; the caller
    /// then continues reading normal lines.
    ///
    /// # Errors
    ///
    /// Fails closed when no literal is pending, when too many bytes arrive, or
    /// when the literal is not UTF-8.
    pub fn feed_literal(&mut self, bytes: &[u8]) -> Result<Option<ImapEvent>> {
        let Some(literal) = &mut self.literal else {
            return Err(Error::unexpected(PROTOCOL, "literal bytes"));
        };
        if literal.collected.len() + bytes.len() > literal.size {
            return Err(Error::malformed(
                PROTOCOL,
                "literal",
                "more bytes than declared",
            ));
        }
        literal.collected.extend_from_slice(bytes);
        if literal.collected.len() < literal.size {
            return Ok(None);
        }
        let text = String::from_utf8(core::mem::take(&mut literal.collected)).map_err(|_| {
            Error::InvalidUtf8 {
                protocol: PROTOCOL,
                field: "literal",
            }
        })?;
        self.literal = None;
        Ok(Some(ImapEvent::Untagged(text)))
    }

    fn parse_line(&mut self, line: &str) -> Result<ImapEvent> {
        if let Some(rest) = line.strip_prefix("* ") {
            let mut words = rest.splitn(2, ' ');
            let keyword = words.next().unwrap_or_default();
            let supplied = words.next().unwrap_or("");
            if keyword.eq_ignore_ascii_case("BYE") {
                self.state = ImapState::Closed;
                return Ok(ImapEvent::Bye);
            }
            if keyword.eq_ignore_ascii_case("CAPABILITY") {
                return Ok(ImapEvent::Capabilities(parse_capabilities(
                    supplied,
                    &self.limits,
                )));
            }
            if keyword.eq_ignore_ascii_case("PREAUTH") {
                self.state = ImapState::Authenticated;
                return Ok(ImapEvent::Greeting {
                    preauthenticated: true,
                });
            }
            if keyword.eq_ignore_ascii_case("OK") {
                if self.state == ImapState::AwaitGreeting {
                    self.state = ImapState::NotAuthenticated;
                    return Ok(ImapEvent::Greeting {
                        preauthenticated: false,
                    });
                }
                return Ok(ImapEvent::Untagged(rest.to_owned()));
            }
            return Ok(ImapEvent::Untagged(rest.to_owned()));
        }
        if let Some(rest) = line.strip_prefix('+') {
            let text = rest.trim_start().to_owned();
            return Ok(ImapEvent::AuthContinue(text));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(Error::unexpected(
                PROTOCOL,
                "response without a pending command",
            ));
        };
        let mut parts = line.splitn(3, ' ');
        let tag = parts.next().unwrap_or_default();
        let status_token = parts
            .next()
            .ok_or_else(|| Error::malformed(PROTOCOL, "tagged response", "missing status"))?;
        let text = parts.next().unwrap_or("").to_owned();
        if tag != pending.tag {
            return Err(Error::malformed(
                PROTOCOL,
                "tag",
                "response tag does not match",
            ));
        }
        let status = ImapStatus::parse(status_token)?;
        self.pending = None;
        if status == ImapStatus::Ok {
            match pending.kind {
                ImapCommandKind::Login
                | ImapCommandKind::AuthenticatePlain
                | ImapCommandKind::AuthContinue => self.state = ImapState::Authenticated,
                ImapCommandKind::StartTls => self.state = ImapState::TlsRequired,
                ImapCommandKind::Logout => self.state = ImapState::Closed,
                ImapCommandKind::Capability | ImapCommandKind::AuthenticateLogin => {}
            }
        } else if pending.kind == ImapCommandKind::Logout {
            self.state = ImapState::Closed;
        }
        Ok(ImapEvent::Tagged {
            tag: tag.to_owned(),
            status,
            text,
        })
    }

    /// Mark the STARTTLS handshake as completed.
    ///
    /// # Errors
    ///
    /// Fails closed unless `STARTTLS` was accepted.
    pub fn tls_established(&mut self) -> Result<()> {
        if self.state != ImapState::TlsRequired {
            return Err(Error::transition(
                PROTOCOL,
                "not-tls-required",
                "tls-established",
            ));
        }
        self.state = ImapState::NotAuthenticated;
        Ok(())
    }
}

/// Parse a space-separated capability list, uppercased and bounded.
pub fn parse_capabilities(input: &str, limits: &Limits) -> Vec<String> {
    let limits = limits.sanitized();
    let mut capabilities = Vec::new();
    for token in input.split_whitespace() {
        if capabilities.len() >= limits.max_capabilities {
            break;
        }
        if token.len() > 64 {
            continue;
        }
        capabilities.push(token.to_ascii_uppercase());
    }
    capabilities
}

fn trailing_literal_size(line: &str) -> Result<Option<usize>> {
    let Some(start) = line.rfind('{') else {
        return Ok(None);
    };
    if !line.ends_with('}') && !line.ends_with("+}") {
        return Ok(None);
    }
    let inner = &line[start + 1..line.len() - 1 - usize::from(line.ends_with("+}"))];
    let digits = inner.strip_suffix('+').unwrap_or(inner);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::malformed(PROTOCOL, "literal", "invalid size"));
    }
    let size = digits
        .parse::<usize>()
        .map_err(|_| Error::malformed(PROTOCOL, "literal", "size overflows"))?;
    Ok(Some(size))
}

fn validate_tag(tag: &str, limits: &Limits) -> Result<()> {
    if tag.is_empty() || tag.len() > 32 {
        return Err(Error::malformed(PROTOCOL, "tag", "invalid length"));
    }
    if !tag
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(Error::malformed(PROTOCOL, "tag", "invalid character"));
    }
    let _ = limits;
    Ok(())
}

fn quote_string(protocol: &'static str, value: &str, limits: &Limits) -> Result<String> {
    if value.len() > limits.max_credential_bytes {
        return Err(Error::limit(
            protocol,
            "credential",
            limits.max_credential_bytes,
        ));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
    {
        return Err(Error::malformed(
            protocol,
            "quoted string",
            "forbidden byte",
        ));
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        if character == '"' || character == '\\' {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    Ok(quoted)
}

fn check_line(line: &str, limits: &Limits) -> Result<()> {
    if line.len() > limits.max_line_bytes {
        return Err(Error::LineTooLong {
            protocol: PROTOCOL,
            maximum: limits.max_line_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_capability_and_login_dialog_reaches_authenticated() {
        let limits = Limits::default();
        let mut client = ImapClient::new(&limits);
        assert!(matches!(
            client
                .on_line("* OK [CAPABILITY IMAP4rev1 STARTTLS] ready")
                .expect("greeting"),
            ImapEvent::Greeting { .. }
        ));
        let tag = client.next_tag().expect("tag");
        let capability = client.capability(&tag).expect("capability");
        assert_eq!(capability, format!("{tag} CAPABILITY\r\n").as_bytes());
        let event = client
            .on_line("* CAPABILITY IMAP4rev1 AUTH=PLAIN AUTH=LOGIN")
            .expect("capability response");
        assert!(
            matches!(event, ImapEvent::Capabilities(items) if items.contains(&"AUTH=PLAIN".to_owned()))
        );
        let event = client
            .on_line(&format!("{tag} OK capability done"))
            .expect("tagged");
        assert!(matches!(
            event,
            ImapEvent::Tagged {
                status: ImapStatus::Ok,
                ..
            }
        ));
        let tag = client.next_tag().expect("tag");
        let login = client.login(&tag, "alice", "p a s s").expect("login");
        assert_eq!(
            login,
            format!("{tag} LOGIN \"alice\" \"p a s s\"\r\n").as_bytes()
        );
        let event = client
            .on_line(&format!("{tag} OK logged in"))
            .expect("reply");
        assert!(matches!(
            event,
            ImapEvent::Tagged {
                status: ImapStatus::Ok,
                ..
            }
        ));
        assert_eq!(client.state(), ImapState::Authenticated);
    }

    #[test]
    fn starttls_and_literal_boundaries_are_enforced() {
        let limits = Limits {
            max_literal_bytes: 8,
            ..Limits::default()
        };
        let mut client = ImapClient::new(&limits);
        client.on_line("* OK ready").expect("greeting");
        let tag = client.next_tag().expect("tag");
        client.starttls(&tag).expect("starttls");
        let event = client
            .on_line(&format!("{tag} OK begin TLS"))
            .expect("reply");
        assert!(matches!(
            event,
            ImapEvent::Tagged {
                status: ImapStatus::Ok,
                ..
            }
        ));
        assert_eq!(client.state(), ImapState::TlsRequired);
        client.tls_established().expect("tls");
        let tag = client.next_tag().expect("tag");
        client.authenticate_login(&tag).expect("auth login");
        client.on_line("+ VXNlcm5hbWU6").expect("continuation");
        client.auth_response("YWxpY2U=").expect("response");
        let event = client.on_line("* 1 FETCH (BODY {8}").expect("literal");
        assert!(matches!(event, ImapEvent::LiteralRequired { size: 8 }));
        assert!(matches!(
            client.on_line("next line"),
            Err(Error::Unexpected { .. })
        ));
        assert!(client.feed_literal(b"12345678").expect("literal").is_some());
    }

    #[test]
    fn bye_bad_tag_and_oversized_lines_fail_closed() {
        let limits = Limits::default();
        let mut client = ImapClient::new(&limits);
        client.on_line("* OK ready").expect("greeting");
        let tag = client.next_tag().expect("tag");
        client.capability(&tag).expect("capability");
        assert!(client
            .on_line("* BYE shutting down")
            .is_ok_and(|event| event == ImapEvent::Bye));
        assert_eq!(client.state(), ImapState::Closed);
        assert!(client.logout("x").is_err());

        let mut client = ImapClient::new(&limits);
        client.on_line("* OK ready").expect("greeting");
        let tag = client.next_tag().expect("tag");
        client.capability(&tag).expect("capability");
        assert!(matches!(
            client.on_line("WRONG NO nope"),
            Err(Error::Malformed { .. })
        ));
        let mut client = ImapClient::new(&Limits {
            max_line_bytes: 4,
            ..Limits::default()
        });
        assert!(matches!(
            client.on_line("* much too long"),
            Err(Error::LineTooLong { .. })
        ));
    }

    #[test]
    fn quoted_strings_escape_backslash_and_quote() {
        let limits = Limits::default();
        let mut client = ImapClient::new(&limits);
        client.on_line("* OK ready").expect("greeting");
        let tag = client.next_tag().expect("tag");
        let line = client.login(&tag, "a\\b", "c\"d").expect("login");
        let text = String::from_utf8(line).expect("utf8");
        assert!(text.contains("\"a\\\\b\" \"c\\\"d\""), "{text}");
    }
}
