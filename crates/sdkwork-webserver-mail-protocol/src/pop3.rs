//! POP3 client-side dialog state machine (nginx mail proxy → backend).
//!
//! The frontend relays client `USER`/`PASS`/`APOP`/`AUTH`/`STLS`/`CAPA` commands
//! to the backend and answers with `+OK`/`-ERR`; this module owns the bounded
//! command builders, the reply parser, the multiline `CAPA` reader and the
//! STARTTLS boundary. It performs no I/O.

use crate::dot_stuffing::DotStuffingReader;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::line::encode_line;

const PROTOCOL: &str = "pop3";

/// POP3 dialog state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pop3State {
    /// Waiting for the `+OK` greeting.
    AwaitGreeting,
    /// Greeting accepted; authorization commands are allowed.
    Authorization,
    /// `PASS`/`APOP`/`AUTH` accepted; transaction commands are allowed.
    Authenticated,
    /// `STLS` accepted; the caller must now run the TLS handshake.
    TlsRequired,
    /// The session is closed (QUIT or failure).
    Closed,
}

/// A POP3 command the frontend sent to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pop3Command {
    /// `USER <name>`
    User,
    /// `PASS <password>`
    Pass,
    /// `APOP <name> <digest>`
    Apop,
    /// `STLS`
    Stls,
    /// `CAPA` (multiline response)
    Capa,
    /// `AUTH PLAIN <initial-response>`
    AuthPlain,
    /// `AUTH LOGIN`
    AuthLogin,
    /// A base64 LOGIN continuation response.
    AuthContinue,
    /// `QUIT`
    Quit,
}

/// One parsed POP3 server reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pop3Reply {
    /// `+OK` (single-line or the first line of a multiline response).
    Ok(String),
    /// `-ERR <message>`.
    Err(String),
    /// `CAPA` payload (multiline dot-stuffed lines, terminator consumed).
    Capa(Vec<String>),
    /// A server continuation line (`+ <base64 salt>`).
    AuthContinue(String),
    /// The dialog reached an authenticated transaction state.
    Authenticated,
    /// The backend accepted STARTTLS.
    TlsRequired,
    /// The `+OK` greeting was accepted.
    Greeting {
        /// Server timestamp usable as the APOP digest salt (possibly empty).
        apop_timestamp: String,
    },
    /// The backend closed the dialog (`+OK` to QUIT).
    Closed,
}

/// Bounded POP3 client dialog.
#[derive(Debug, Clone)]
pub struct Pop3Client {
    limits: Limits,
    state: Pop3State,
    pending: Option<Pop3Command>,
    capa: Option<DotStuffingReader>,
}

impl Pop3Client {
    /// Create a client awaiting the greeting (plaintext or implicit TLS).
    pub fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.sanitized(),
            state: Pop3State::AwaitGreeting,
            pending: None,
            capa: None,
        }
    }

    /// Current state.
    pub fn state(&self) -> Pop3State {
        self.state
    }

    /// Bounds in effect.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Parse the server greeting and extract a possible APOP timestamp.
    ///
    /// # Errors
    ///
    /// Fails closed on `-ERR`, malformed or oversized greetings, or a duplicate
    /// greeting.
    pub fn greeting(&mut self, line: &str) -> Result<Pop3Reply> {
        if self.state != Pop3State::AwaitGreeting {
            return Err(Error::unexpected(PROTOCOL, "greeting"));
        }
        let (positive, text) = split_status(line)?;
        if !positive {
            self.state = Pop3State::Closed;
            return Err(Error::unexpected(PROTOCOL, "greeting rejected"));
        }
        self.state = Pop3State::Authorization;
        Ok(Pop3Reply::Greeting {
            apop_timestamp: text.trim().to_owned(),
        })
    }

    fn issue(&mut self, command: Pop3Command, line: &str) -> Result<Vec<u8>> {
        let allowed = match command {
            Pop3Command::User
            | Pop3Command::Pass
            | Pop3Command::Apop
            | Pop3Command::AuthPlain
            | Pop3Command::AuthLogin => {
                matches!(
                    self.state,
                    Pop3State::Authorization | Pop3State::TlsRequired
                )
            }
            Pop3Command::AuthContinue => self.pending == Some(Pop3Command::AuthLogin),
            Pop3Command::Capa => matches!(
                self.state,
                Pop3State::Authorization | Pop3State::Authenticated | Pop3State::TlsRequired
            ),
            Pop3Command::Stls => matches!(self.state, Pop3State::Authorization),
            Pop3Command::Quit => !matches!(self.state, Pop3State::Closed),
        };
        if !allowed || self.pending.is_some() {
            return Err(Error::unexpected(PROTOCOL, "command in this state"));
        }
        self.pending = Some(command);
        encode_line(line, &self.limits)
    }

    /// Build `USER <name>`.
    ///
    /// # Errors
    ///
    /// Fails closed in the wrong state or for a credential with forbidden bytes.
    pub fn user(&mut self, name: &str) -> Result<Vec<u8>> {
        validate_token(name, &self.limits)?;
        self.issue(Pop3Command::User, &format!("USER {name}"))
    }

    /// Build `PASS <password>`.
    ///
    /// # Errors
    ///
    /// Fails closed in the wrong state or for a forbidden password byte.
    pub fn pass(&mut self, password: &str) -> Result<Vec<u8>> {
        validate_token(password, &self.limits)?;
        self.issue(Pop3Command::Pass, &format!("PASS {password}"))
    }

    /// Build `APOP <name> <hex-digest>` (32 lowercase hex characters).
    ///
    /// # Errors
    ///
    /// Fails closed for an invalid digest, forbidden name byte, or wrong state.
    pub fn apop(&mut self, name: &str, digest: &str) -> Result<Vec<u8>> {
        validate_token(name, &self.limits)?;
        if digest.len() != 32 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::malformed(
                PROTOCOL,
                "APOP digest",
                "expected 32 hex characters",
            ));
        }
        self.issue(Pop3Command::Apop, &format!("APOP {name} {digest}"))
    }

    /// Build `STLS`.
    ///
    /// # Errors
    ///
    /// Fails closed unless the session is in the authorization state.
    pub fn stls(&mut self) -> Result<Vec<u8>> {
        self.issue(Pop3Command::Stls, "STLS")
    }

    /// Build `CAPA`.
    ///
    /// # Errors
    ///
    /// Fails closed in a state where `CAPA` is not valid.
    pub fn capa(&mut self) -> Result<Vec<u8>> {
        self.issue(Pop3Command::Capa, "CAPA")
    }

    /// Build `AUTH PLAIN <base64>`.
    ///
    /// # Errors
    ///
    /// Fails closed for an unsupported state or an oversized auth line.
    pub fn auth_plain(&mut self, initial_response: &str) -> Result<Vec<u8>> {
        auth_line(&self.limits, initial_response)?;
        self.issue(
            Pop3Command::AuthPlain,
            &format!("AUTH PLAIN {initial_response}"),
        )
    }

    /// Build `AUTH LOGIN`.
    ///
    /// # Errors
    ///
    /// Fails closed in a state where `AUTH` is not valid.
    pub fn auth_login(&mut self) -> Result<Vec<u8>> {
        self.issue(Pop3Command::AuthLogin, "AUTH LOGIN")
    }

    /// Build a base64 LOGIN continuation response.
    ///
    /// # Errors
    ///
    /// Fails closed unless an `AUTH LOGIN` reply is pending.
    pub fn auth_login_response(&mut self, response: &str) -> Result<Vec<u8>> {
        auth_line(&self.limits, response)?;
        self.issue(Pop3Command::AuthContinue, response)
    }

    /// Build `QUIT`.
    ///
    /// # Errors
    ///
    /// Fails closed after the session was closed.
    pub fn quit(&mut self) -> Result<Vec<u8>> {
        self.issue(Pop3Command::Quit, "QUIT")
    }

    /// Consume one backend reply line.
    ///
    /// Returns [`Pop3Reply::Capa`] only after the multiline terminator; call
    /// this repeatedly while collecting capability lines.
    ///
    /// # Errors
    ///
    /// Fails closed on out-of-order replies, multiline overruns, or malformed
    /// status lines.
    pub fn on_line(&mut self, line: &str) -> Result<Pop3Reply> {
        check_line(PROTOCOL, line, &self.limits)?;
        if let Some(reader) = &mut self.capa {
            let complete = reader.feed(format!("{line}\r\n").as_bytes())?;
            let _ = complete;
            if reader.is_complete() {
                let lines = reader.lines().to_vec();
                self.capa = None;
                self.pending = None;
                return Ok(Pop3Reply::Capa(lines));
            }
            return Ok(Pop3Reply::Ok(String::new()));
        }
        let (positive, text) = split_status(line)?;
        let Some(command) = self.pending.take() else {
            return Err(Error::unexpected(
                PROTOCOL,
                "reply without a pending command",
            ));
        };
        match command {
            Pop3Command::Capa => {
                if !positive {
                    self.state = Pop3State::Closed;
                    return Ok(Pop3Reply::Err(text));
                }
                // The `+OK` line is the capability-list status line, not payload.
                self.capa = Some(DotStuffingReader::new(PROTOCOL, &self.limits));
                Ok(Pop3Reply::Ok(text))
            }
            Pop3Command::User => {
                if !positive {
                    return Ok(Pop3Reply::Err(text));
                }
                Ok(Pop3Reply::Ok(text))
            }
            Pop3Command::Pass | Pop3Command::Apop | Pop3Command::AuthPlain => {
                if positive {
                    self.state = Pop3State::Authenticated;
                    Ok(Pop3Reply::Authenticated)
                } else {
                    Ok(Pop3Reply::Err(text))
                }
            }
            Pop3Command::AuthLogin => {
                if positive {
                    Ok(Pop3Reply::AuthContinue(text))
                } else {
                    Ok(Pop3Reply::Err(text))
                }
            }
            Pop3Command::AuthContinue => {
                if positive {
                    self.state = Pop3State::Authenticated;
                    Ok(Pop3Reply::Authenticated)
                } else {
                    Ok(Pop3Reply::Err(text))
                }
            }
            Pop3Command::Stls => {
                if positive {
                    self.state = Pop3State::TlsRequired;
                    Ok(Pop3Reply::TlsRequired)
                } else {
                    Ok(Pop3Reply::Err(text))
                }
            }
            Pop3Command::Quit => {
                self.state = Pop3State::Closed;
                Ok(Pop3Reply::Closed)
            }
        }
    }

    /// Mark the STARTTLS handshake as completed so authorization can continue.
    ///
    /// # Errors
    ///
    /// Fails closed unless the backend accepted `STLS`.
    pub fn tls_established(&mut self) -> Result<()> {
        if self.state != Pop3State::TlsRequired {
            return Err(Error::transition(
                PROTOCOL,
                "not-tls-required",
                "tls-established",
            ));
        }
        self.state = Pop3State::Authorization;
        Ok(())
    }
}

fn split_status(line: &str) -> Result<(bool, String)> {
    if let Some(rest) = line.strip_prefix("+OK") {
        Ok((true, rest.trim_start().to_owned()))
    } else if let Some(rest) = line.strip_prefix("-ERR") {
        Ok((false, rest.trim_start().to_owned()))
    } else if let Some(rest) = line.strip_prefix('+') {
        Ok((true, rest.trim_start().to_owned()))
    } else {
        Err(Error::malformed(PROTOCOL, "status", "expected +OK or -ERR"))
    }
}

fn validate_token(value: &str, limits: &Limits) -> Result<()> {
    if value.is_empty() || value.len() > limits.max_credential_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "credential",
            limits.max_credential_bytes,
        ));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n' || byte == b' ')
    {
        return Err(Error::malformed(PROTOCOL, "credential", "forbidden byte"));
    }
    Ok(())
}

fn auth_line(limits: &Limits, line: &str) -> Result<()> {
    if line.len() > limits.max_auth_line_bytes {
        return Err(Error::limit(
            PROTOCOL,
            "auth line",
            limits.max_auth_line_bytes,
        ));
    }
    if line
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
    {
        return Err(Error::malformed(PROTOCOL, "auth line", "forbidden byte"));
    }
    Ok(())
}

fn check_line(protocol: &'static str, line: &str, limits: &Limits) -> Result<()> {
    if line.len() > limits.max_line_bytes {
        return Err(Error::LineTooLong {
            protocol,
            maximum: limits.max_line_bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_user_pass_dialog_reaches_the_transaction_state() {
        let limits = Limits::default();
        let mut client = Pop3Client::new(&limits);
        let greeting = client
            .greeting("+OK POP3 ready <1896.697170952@mail>")
            .expect("greeting");
        assert!(matches!(greeting, Pop3Reply::Greeting { .. }));
        let user = client.user("alice").expect("user");
        assert_eq!(user, b"USER alice\r\n");
        assert!(matches!(
            client.on_line("+OK user accepted").expect("reply"),
            Pop3Reply::Ok(_)
        ));
        let pass = client.pass("secret").expect("pass");
        assert_eq!(pass, b"PASS secret\r\n");
        assert!(matches!(
            client.on_line("+OK mailbox locked").expect("reply"),
            Pop3Reply::Authenticated
        ));
        assert_eq!(client.state(), Pop3State::Authenticated);
    }

    #[test]
    fn auth_failure_keeps_the_authorization_state() {
        let limits = Limits::default();
        let mut client = Pop3Client::new(&limits);
        client.greeting("+OK ready").expect("greeting");
        client.user("alice").expect("user");
        client.on_line("+OK").expect("user reply");
        client.pass("wrong").expect("pass");
        assert!(matches!(
            client.on_line("-ERR invalid credentials").expect("reply"),
            Pop3Reply::Err(message) if message == "invalid credentials"
        ));
        assert_eq!(client.state(), Pop3State::Authorization);
    }

    #[test]
    fn stls_and_capa_multiline_are_bounded_and_ordered() {
        let limits = Limits::default();
        let mut client = Pop3Client::new(&limits);
        client.greeting("+OK ready").expect("greeting");
        client.stls().expect("stls");
        assert!(matches!(
            client.on_line("+OK begin TLS negotiation").expect("reply"),
            Pop3Reply::TlsRequired
        ));
        assert_eq!(client.state(), Pop3State::TlsRequired);
        client.tls_established().expect("tls");
        client
            .user("alice")
            .expect("authorization resumes after TLS");
        client.on_line("+OK").expect("user reply");
        client.capa().expect("capa");
        client
            .on_line("+OK capability list follows")
            .expect("capa ok");
        client.on_line("USER").expect("line");
        client.on_line("SASL PLAIN LOGIN").expect("line");
        assert!(matches!(
            client.on_line(".").expect("terminator"),
            Pop3Reply::Capa(lines) if lines == ["USER", "SASL PLAIN LOGIN"]
        ));
    }

    #[test]
    fn out_of_state_and_malformed_inputs_fail_closed() {
        let limits = Limits::default();
        let mut client = Pop3Client::new(&limits);
        assert!(client.pass("x").is_err());
        client.greeting("+OK ready").expect("greeting");
        client.user("alice").expect("user");
        assert!(client.stls().is_err(), "command while a reply is pending");
        client.on_line("+OK").expect("user reply");
        assert!(client.on_line("garbage").is_err());
        client
            .greeting("+OK again")
            .expect_err("duplicate greeting");
    }

    #[test]
    fn oversized_lines_and_credentials_fail_closed() {
        let limits = Limits {
            max_line_bytes: 4,
            max_credential_bytes: 4,
            ..Limits::default()
        };
        let mut client = Pop3Client::new(&limits);
        client.greeting("+OK ready").expect("greeting");
        assert!(client.user("toolonguser").is_err());
        assert!(matches!(
            client.on_line("too long line"),
            Err(Error::LineTooLong { .. })
        ));
    }
}
