//! SMTP client-side dialog state machine (nginx mail proxy → backend).
//!
//! Covers the greeting/EHLO capability negotiation, optional `STARTTLS`,
//! `AUTH PLAIN`/`AUTH LOGIN`, `XCLIENT`, `MAIL FROM`/`RCPT TO`, pipelined
//! envelope commands, and bounded dot-stuffed `DATA`. No I/O.

use std::collections::VecDeque;

use crate::auth::{encode_plain, LoginDialog};
use crate::dot_stuffing::encode_multiline;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::line::encode_line;
use crate::xclient::XclientRequest;

const PROTOCOL: &str = "smtp";

/// SMTP dialog state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpState {
    /// Waiting for the `220` greeting.
    AwaitGreeting,
    /// Greeting accepted; envelope/auth commands are allowed.
    Ready,
    /// `STARTTLS` accepted; the caller must run the TLS handshake.
    TlsRequired,
    /// `DATA` accepted (`354`); the dot-stuffed payload may be sent.
    AwaitDataEnd,
    /// Session closed (`QUIT`).
    Closed,
}

/// One SMTP command sent to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpCommand {
    /// `EHLO`
    Ehlo,
    /// `HELO`
    Helo,
    /// `STARTTLS`
    StartTls,
    /// `AUTH PLAIN <initial-response>`
    AuthPlain,
    /// `AUTH LOGIN`
    AuthLogin,
    /// A base64 auth continuation.
    AuthContinue,
    /// `XCLIENT`
    Xclient,
    /// `MAIL FROM`
    MailFrom,
    /// `RCPT TO`
    RcptTo,
    /// `DATA`
    Data,
    /// End of the dot-stuffed `DATA` payload.
    DataEnd,
    /// `QUIT`
    Quit,
}

/// One parsed SMTP reply (possibly multiline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpReply {
    /// Three-digit status code.
    pub code: u16,
    /// Reply lines (first line included; continuation markers stripped).
    pub lines: Vec<String>,
    /// Parsed RFC 3463 enhanced status code (`2.1.0`).
    pub enhanced: Option<(u8, u8, u8)>,
}

impl SmtpReply {
    /// Whether the code is a positive completion (2xx).
    pub fn is_positive(&self) -> bool {
        (200..300).contains(&self.code)
    }

    /// Whether the code requests an auth/data continuation (334/354).
    pub fn is_continuation(&self) -> bool {
        matches!(self.code, 334 | 354)
    }

    /// Whether the code is a permanent failure (5xx).
    pub fn is_permanent_failure(&self) -> bool {
        (500..600).contains(&self.code)
    }
}

/// One command coalesced into an SMTP pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtpPipelineCommand {
    /// `MAIL FROM:<path>`
    MailFrom(String),
    /// `RCPT TO:<path>`
    RcptTo(String),
}

/// Bounded SMTP client dialog.
#[derive(Debug, Clone)]
pub struct SmtpClient {
    limits: Limits,
    state: SmtpState,
    pending: VecDeque<SmtpCommand>,
    capabilities: Vec<String>,
    login: Option<LoginDialog>,
    plain_line: Option<String>,
    greeted: bool,
    multi: Option<SmtpReply>,
}

impl SmtpClient {
    /// Create a client awaiting the greeting.
    pub fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            state: SmtpState::AwaitGreeting,
            pending: VecDeque::new(),
            capabilities: Vec::new(),
            login: None,
            plain_line: None,
            greeted: false,
            multi: None,
        }
    }

    /// Current state.
    pub fn state(&self) -> SmtpState {
        self.state
    }

    /// Capabilities advertised by the last `EHLO`.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// Whether the backend advertised `token` in the last `EHLO`.
    pub fn has_capability(&self, token: &str) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability.eq_ignore_ascii_case(token))
    }

    fn enqueue(&mut self, command: SmtpCommand) -> Result<()> {
        if self.pending.len() >= self.limits.max_pipeline_commands {
            return Err(Error::limit(
                PROTOCOL,
                "pipelined commands",
                self.limits.max_pipeline_commands,
            ));
        }
        self.pending.push_back(command);
        Ok(())
    }

    /// Build and queue `EHLO <domain>`.
    ///
    /// # Errors
    ///
    /// Fails closed in the wrong state or for an invalid domain.
    pub fn ehlo(&mut self, domain: &str) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "EHLO in this state"));
        }
        validate_domain(domain, &self.limits)?;
        self.enqueue(SmtpCommand::Ehlo)?;
        encode_line(&format!("EHLO {domain}"), &self.limits)
    }

    /// Build and queue `HELO <domain>`.
    ///
    /// # Errors
    ///
    /// Fails closed in the wrong state or for an invalid domain.
    pub fn helo(&mut self, domain: &str) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "HELO in this state"));
        }
        validate_domain(domain, &self.limits)?;
        self.enqueue(SmtpCommand::Helo)?;
        encode_line(&format!("HELO {domain}"), &self.limits)
    }

    /// Build and queue `STARTTLS`.
    ///
    /// # Errors
    ///
    /// Fails closed unless the session is ready.
    pub fn starttls(&mut self) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "STARTTLS in this state"));
        }
        self.enqueue(SmtpCommand::StartTls)?;
        encode_line("STARTTLS", &self.limits)
    }

    /// Build and queue `AUTH PLAIN <base64>`.
    ///
    /// # Errors
    ///
    /// Fails closed for oversized/forbidden credentials or the wrong state.
    pub fn auth_plain(&mut self, user: &str, password: &str) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "AUTH PLAIN in this state"));
        }
        let initial = encode_plain(user, password, &self.limits)?;
        self.plain_line = Some(initial.clone());
        self.enqueue(SmtpCommand::AuthPlain)?;
        encode_line(&format!("AUTH PLAIN {initial}"), &self.limits)
    }

    /// Build and queue `AUTH LOGIN`.
    ///
    /// # Errors
    ///
    /// Fails closed for oversized/forbidden credentials or the wrong state.
    pub fn auth_login(&mut self, user: &str, password: &str) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "AUTH LOGIN in this state"));
        }
        self.login = Some(LoginDialog::new(user, password, &self.limits)?);
        self.enqueue(SmtpCommand::AuthLogin)?;
        encode_line("AUTH LOGIN", &self.limits)
    }

    /// Build the next auth continuation line after a `334` reply.
    ///
    /// # Errors
    ///
    /// Fails closed when no auth continuation is expected.
    pub fn auth_continuation(&mut self) -> Result<Vec<u8>> {
        let line = if let Some(plain) = &self.plain_line {
            plain.clone()
        } else if let Some(dialog) = &mut self.login {
            dialog
                .on_prompt()?
                .ok_or_else(|| Error::unexpected(PROTOCOL, "auth continuation"))?
                .to_owned()
        } else {
            return Err(Error::unexpected(PROTOCOL, "auth continuation"));
        };
        self.enqueue(SmtpCommand::AuthContinue)?;
        encode_line(&line, &self.limits)
    }

    /// Build and queue `XCLIENT` with the nginx-compatible attribute line.
    ///
    /// # Errors
    ///
    /// Fails closed in the wrong state or for invalid attributes.
    pub fn xclient(&mut self, request: &XclientRequest) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "XCLIENT in this state"));
        }
        let line = request.encode_line(&self.limits)?;
        self.enqueue(SmtpCommand::Xclient)?;
        encode_line(&line, &self.limits)
    }

    /// Build and queue `MAIL FROM:<path>`.
    ///
    /// # Errors
    ///
    /// Fails closed for an invalid path or wrong state.
    pub fn mail_from(&mut self, path: &str) -> Result<Vec<u8>> {
        if !matches!(self.state, SmtpState::Ready) || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "MAIL FROM in this state"));
        }
        validate_path(path, &self.limits)?;
        self.enqueue(SmtpCommand::MailFrom)?;
        encode_line(&format!("MAIL FROM:<{path}>"), &self.limits)
    }

    /// Build and queue `RCPT TO:<path>`.
    ///
    /// # Errors
    ///
    /// Fails closed for an invalid path or wrong state.
    pub fn rcpt_to(&mut self, path: &str) -> Result<Vec<u8>> {
        if !matches!(self.state, SmtpState::Ready) || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "RCPT TO in this state"));
        }
        validate_path(path, &self.limits)?;
        self.enqueue(SmtpCommand::RcptTo)?;
        encode_line(&format!("RCPT TO:<{path}>"), &self.limits)
    }

    /// Build a pipelined `MAIL FROM` + `RCPT TO...` batch.
    ///
    /// Replies are consumed one per [`Self::on_line`] call in order.
    ///
    /// # Errors
    ///
    /// Fails closed outside `Ready`, with pending replies, or with invalid
    /// paths/count.
    pub fn pipeline(&mut self, commands: &[SmtpPipelineCommand]) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "pipeline in this state"));
        }
        if commands.is_empty() || commands.len() > self.limits.max_pipeline_commands {
            return Err(Error::limit(
                PROTOCOL,
                "pipelined commands",
                self.limits.max_pipeline_commands,
            ));
        }
        let mut encoded = Vec::new();
        for command in commands {
            let line = match command {
                SmtpPipelineCommand::MailFrom(path) => {
                    validate_path(path, &self.limits)?;
                    self.enqueue(SmtpCommand::MailFrom)?;
                    format!("MAIL FROM:<{path}>")
                }
                SmtpPipelineCommand::RcptTo(path) => {
                    validate_path(path, &self.limits)?;
                    self.enqueue(SmtpCommand::RcptTo)?;
                    format!("RCPT TO:<{path}>")
                }
            };
            encoded.extend_from_slice(&encode_line(&line, &self.limits)?);
        }
        Ok(encoded)
    }

    /// Build and queue `DATA`.
    ///
    /// # Errors
    ///
    /// Fails closed outside `Ready`.
    pub fn data(&mut self) -> Result<Vec<u8>> {
        if self.state != SmtpState::Ready || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "DATA in this state"));
        }
        self.enqueue(SmtpCommand::Data)?;
        encode_line("DATA", &self.limits)
    }

    /// Encode a dot-stuffed `DATA` payload ending in the `CRLF.CRLF` terminator.
    ///
    /// # Errors
    ///
    /// Fails closed outside `AwaitDataEnd`, for embedded control bytes, or when
    /// the payload exceeds `limits.max_smtp_data_bytes`.
    pub fn data_body<'a, I>(&mut self, lines: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = &'a str>,
    {
        if self.state != SmtpState::AwaitDataEnd || !self.pending.is_empty() {
            return Err(Error::unexpected(PROTOCOL, "DATA body in this state"));
        }
        let encoded = encode_multiline(lines, &self.limits)?;
        if encoded.len() > self.limits.max_smtp_data_bytes {
            return Err(Error::limit(
                PROTOCOL,
                "DATA bytes",
                self.limits.max_smtp_data_bytes,
            ));
        }
        self.enqueue(SmtpCommand::DataEnd)?;
        Ok(encoded)
    }

    /// Build and queue `QUIT`.
    ///
    /// # Errors
    ///
    /// Fails closed after the session closed.
    pub fn quit(&mut self) -> Result<Vec<u8>> {
        if self.state == SmtpState::Closed {
            return Err(Error::unexpected(PROTOCOL, "QUIT after close"));
        }
        self.enqueue(SmtpCommand::Quit)?;
        encode_line("QUIT", &self.limits)
    }

    /// Consume one backend reply line, accumulating multiline replies.
    ///
    /// Returns `None` while a multiline reply is incomplete.
    ///
    /// # Errors
    ///
    /// Fails closed on malformed/oversized/out-of-order replies and on an
    /// invalid state transition for the replied command.
    pub fn on_line(&mut self, line: &str) -> Result<Option<SmtpReply>> {
        if line.len() > self.limits.max_line_bytes {
            return Err(Error::LineTooLong {
                protocol: PROTOCOL,
                maximum: self.limits.max_line_bytes,
            });
        }
        let (code, continuation, text) = split_reply_line(line)?;
        if let Some(reply) = self.multi.take() {
            let mut reply = reply;
            reply.lines.push(text);
            if continuation {
                self.multi = Some(reply);
                return Ok(None);
            }
            let command = self
                .pending
                .pop_front()
                .ok_or_else(|| Error::unexpected(PROTOCOL, "reply without a pending command"))?;
            reply.enhanced = reply.lines.last().and_then(|line| enhanced_status(line));
            self.apply_reply(command, &reply)?;
            return Ok(Some(reply));
        }
        let mut reply = SmtpReply {
            code,
            lines: vec![text],
            enhanced: None,
        };
        if continuation {
            self.multi = Some(reply);
            return Ok(None);
        }
        let command = self
            .pending
            .pop_front()
            .ok_or_else(|| Error::unexpected(PROTOCOL, "reply without a pending command"))?;
        reply.enhanced = enhanced_status(&reply.lines[0]);
        self.apply_reply(command, &reply)?;
        Ok(Some(reply))
    }

    fn apply_reply(&mut self, command: SmtpCommand, reply: &SmtpReply) -> Result<()> {
        match command {
            SmtpCommand::Ehlo => {
                if reply.is_positive() {
                    self.capabilities = parse_ehlo(&reply.lines, &self.limits);
                    self.state = SmtpState::Ready;
                    self.greeted = true;
                } else if reply.code == 220 {
                    self.state = SmtpState::Ready;
                }
            }
            SmtpCommand::Helo => {
                if reply.is_positive() {
                    self.capabilities.clear();
                    self.state = SmtpState::Ready;
                    self.greeted = true;
                }
            }
            SmtpCommand::StartTls => {
                if reply.code == 220 {
                    self.state = SmtpState::TlsRequired;
                }
            }
            SmtpCommand::AuthPlain | SmtpCommand::AuthLogin | SmtpCommand::AuthContinue => {
                if reply.code == 235 {
                    self.state = SmtpState::Ready;
                    self.plain_line = None;
                    self.login = None;
                } else if reply.code == 334 {
                    // A continuation is expected; the caller produces the next
                    // line through `auth_continuation`.
                }
            }
            SmtpCommand::Xclient => {
                if reply.code == 220 { /* ready for envelope commands */ }
            }
            SmtpCommand::MailFrom | SmtpCommand::RcptTo => {
                if reply.is_positive() {
                    self.state = SmtpState::Ready;
                }
            }
            SmtpCommand::Data => {
                if reply.code == 354 {
                    self.state = SmtpState::AwaitDataEnd;
                }
            }
            SmtpCommand::DataEnd => {
                if reply.is_positive() {
                    self.state = SmtpState::Ready;
                }
            }
            SmtpCommand::Quit => {
                self.state = SmtpState::Closed;
            }
        }
        Ok(())
    }

    /// Parse and record the greeting without a pending command.
    ///
    /// # Errors
    ///
    /// Fails closed unless the code is `220` and the greeting is still pending.
    pub fn greeting(&mut self, line: &str) -> Result<()> {
        if self.state != SmtpState::AwaitGreeting || self.greeted {
            return Err(Error::unexpected(PROTOCOL, "greeting"));
        }
        let (code, continuation, _) = split_reply_line(line)?;
        if code != 220 || continuation {
            return Err(Error::malformed(
                PROTOCOL,
                "greeting",
                "expected a 220 line",
            ));
        }
        self.state = SmtpState::Ready;
        self.greeted = true;
        Ok(())
    }

    /// Mark the STARTTLS handshake as completed.
    ///
    /// # Errors
    ///
    /// Fails closed unless `STARTTLS` was accepted.
    pub fn tls_established(&mut self) -> Result<()> {
        if self.state != SmtpState::TlsRequired {
            return Err(Error::transition(
                PROTOCOL,
                "not-tls-required",
                "tls-established",
            ));
        }
        self.state = SmtpState::Ready;
        self.capabilities.clear();
        Ok(())
    }
}

fn split_reply_line(line: &str) -> Result<(u16, bool, String)> {
    let bytes = line.as_bytes();
    if bytes.len() < 3 || !bytes[..3].iter().all(u8::is_ascii_digit) {
        return Err(Error::malformed(
            PROTOCOL,
            "reply",
            "expected a three-digit code",
        ));
    }
    let code = line[..3]
        .parse::<u16>()
        .map_err(|_| Error::malformed(PROTOCOL, "reply", "code overflows"))?;
    if bytes.len() == 3 {
        return Ok((code, false, String::new()));
    }
    let separator = bytes[3];
    if separator != b' ' && separator != b'-' {
        return Err(Error::malformed(
            PROTOCOL,
            "reply",
            "expected SP or - after the code",
        ));
    }
    Ok((code, separator == b'-', line[4..].to_owned()))
}

fn enhanced_status(line: &str) -> Option<(u8, u8, u8)> {
    let token = line.split_whitespace().next()?;
    let mut parts = token.split('.');
    let class = parts.next()?.parse::<u8>().ok()?;
    let subject = parts.next()?.parse::<u8>().ok()?;
    let detail = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() || !(2..=5).contains(&class) {
        return None;
    }
    Some((class, subject, detail))
}

fn parse_ehlo(lines: &[String], limits: &Limits) -> Vec<String> {
    let limits = limits.sanitized();
    let mut capabilities = Vec::new();
    for line in lines.iter().skip(1) {
        let token = line.split_whitespace().next().unwrap_or_default();
        if token.is_empty() || token.len() > 64 {
            continue;
        }
        if capabilities.len() >= limits.max_capabilities {
            break;
        }
        capabilities.push(token.to_ascii_uppercase());
    }
    capabilities
}

fn validate_domain(domain: &str, limits: &Limits) -> Result<()> {
    if domain.is_empty() || domain.len() > 255 {
        return Err(Error::malformed(PROTOCOL, "domain", "invalid length"));
    }
    if !domain.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'[' | b']' | b':')
    }) {
        return Err(Error::malformed(PROTOCOL, "domain", "invalid character"));
    }
    let _ = limits;
    Ok(())
}

fn validate_path(path: &str, limits: &Limits) -> Result<()> {
    if path.len() > limits.max_line_bytes {
        return Err(Error::LineTooLong {
            protocol: PROTOCOL,
            maximum: limits.max_line_bytes,
        });
    }
    if path
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n' || byte == b'<' || byte == b'>')
    {
        return Err(Error::malformed(PROTOCOL, "path", "forbidden byte"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_ehlo_and_starttls_boundary() {
        let limits = Limits::default();
        let mut client = SmtpClient::new(&limits);
        client
            .greeting("220 mx.example.com ESMTP")
            .expect("greeting");
        let ehlo = client.ehlo("client.example.com").expect("ehlo");
        assert_eq!(ehlo, b"EHLO client.example.com\r\n");
        assert!(client
            .on_line("250-mx.example.com")
            .expect("line")
            .is_none());
        assert!(client.on_line("250-STARTTLS").expect("line").is_none());
        let reply = client
            .on_line("250 AUTH PLAIN LOGIN")
            .expect("final line")
            .expect("reply");
        assert_eq!(reply.code, 250);
        assert_eq!(reply.lines.len(), 3);
        assert!(client.has_capability("STARTTLS"));
        assert!(client.has_capability("AUTH"));
        client.starttls().expect("starttls");
        let reply = client
            .on_line("220 ready for TLS")
            .expect("reply")
            .expect("reply");
        assert_eq!(reply.code, 220);
        assert_eq!(client.state(), SmtpState::TlsRequired);
        client.tls_established().expect("tls");
        assert_eq!(client.state(), SmtpState::Ready);
    }

    #[test]
    fn auth_login_dialog_and_mail_rcpt_data_flow() {
        let limits = Limits::default();
        let mut client = SmtpClient::new(&limits);
        client.greeting("220 ready").expect("greeting");
        client.ehlo("client").expect("ehlo");
        client
            .on_line("250 AUTH LOGIN")
            .expect("final")
            .expect("reply");
        client.auth_login("alice", "secret").expect("auth login");
        let reply = client
            .on_line("334 VXNlcm5hbWU6")
            .expect("reply")
            .expect("reply");
        assert_eq!(reply.code, 334);
        let user_line = client.auth_continuation().expect("user line");
        assert!(!user_line.is_empty());
        let reply = client
            .on_line("334 UGFzc3dvcmQ6")
            .expect("reply")
            .expect("reply");
        assert_eq!(reply.code, 334);
        let password_line = client.auth_continuation().expect("password line");
        assert!(!password_line.is_empty());
        let reply = client
            .on_line("235 2.7.0 authenticated")
            .expect("reply")
            .expect("reply");
        assert_eq!(reply.enhanced, Some((2, 7, 0)));
        assert_eq!(client.state(), SmtpState::Ready);
        client.mail_from("alice@example.com").expect("mail");
        let reply = client
            .on_line("250 2.1.0 sender ok")
            .expect("reply")
            .expect("reply");
        assert!(reply.is_positive());
        client.rcpt_to("bob@example.com").expect("rcpt");
        let reply = client
            .on_line("250 2.1.5 recipient ok")
            .expect("reply")
            .expect("reply");
        assert!(reply.is_positive());
        client.data().expect("data");
        let reply = client
            .on_line("354 end with .")
            .expect("reply")
            .expect("reply");
        assert_eq!(reply.code, 354);
        let body = client
            .data_body([".leading dot", "plain line"])
            .expect("body");
        assert_eq!(body, b"..leading dot\r\nplain line\r\n.\r\n");
        let reply = client
            .on_line("250 2.0.0 queued")
            .expect("reply")
            .expect("reply");
        assert!(reply.is_positive());
        assert_eq!(client.state(), SmtpState::Ready);
    }

    #[test]
    fn pipeline_replies_and_failures_are_ordered() {
        let limits = Limits::default();
        let mut client = SmtpClient::new(&limits);
        client.greeting("220 ready").expect("greeting");
        let batch = client
            .pipeline(&[
                SmtpPipelineCommand::MailFrom("alice@example.com".to_owned()),
                SmtpPipelineCommand::RcptTo("bob@example.com".to_owned()),
            ])
            .expect("pipeline");
        assert_eq!(
            batch,
            b"MAIL FROM:<alice@example.com>\r\nRCPT TO:<bob@example.com>\r\n"
        );
        let reply = client
            .on_line("250 sender ok")
            .expect("reply")
            .expect("reply");
        assert!(reply.is_positive());
        let reply = client
            .on_line("550 5.1.1 no such user")
            .expect("reply")
            .expect("reply");
        assert!(reply.is_permanent_failure());
        assert_eq!(reply.enhanced, Some((5, 1, 1)));
    }

    #[test]
    fn malformed_and_out_of_state_inputs_fail_closed() {
        let limits = Limits::default();
        let mut client = SmtpClient::new(&limits);
        assert!(client.ehlo("client").is_err());
        assert!(client.data().is_err());
        client.greeting("220 ready").expect("greeting");
        assert!(client.on_line("250 unexpected").is_err());
        assert!(client.on_line("garbage").is_err());
        client.ehlo("client").expect("ehlo");
        client.on_line("250 ok").expect("reply").expect("reply");
        assert!(client.mail_from("bad\r\npath").is_err());
        assert!(matches!(
            client.on_line("250000"),
            Err(Error::Malformed { .. })
        ));
    }

    #[test]
    fn auth_plain_initial_response_round_trips() {
        use crate::auth::decode_plain;
        let limits = Limits::default();
        let mut client = SmtpClient::new(&limits);
        client.greeting("220 ready").expect("greeting");
        client.ehlo("client").expect("ehlo");
        client
            .on_line("250 AUTH PLAIN")
            .expect("reply")
            .expect("reply");
        let line = client.auth_plain("alice", "secret").expect("auth");
        let text = String::from_utf8(line).expect("utf8");
        let encoded = text.trim_end().strip_prefix("AUTH PLAIN ").expect("prefix");
        let credentials = decode_plain(encoded, &limits).expect("decode");
        assert_eq!(credentials.authentication_id, "alice");
    }
}
