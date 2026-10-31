//! Post-auth backend relay glue.
//!
//! After the frontend authenticated the client, nginx opens the backend
//! connection, optionally emits a `XCLIENT`/PROXY header, reads the backend
//! greeting and then relays lines verbatim in both directions. [`BackendRelay`]
//! models that phase: greeting validation, bounded line forwarding, and the
//! bounded backend bootstrap sequence builder.

use crate::error::{Error, Result};
use crate::frontend::MailProtocol;
use crate::limits::Limits;
use crate::line::encode_line;
use crate::xclient::XclientRequest;

/// Relay phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayPhase {
    /// Waiting for the backend greeting line.
    AwaitGreeting,
    /// Greeting accepted; lines may be relayed in both directions.
    Ready,
    /// Session finished.
    Finished,
}

/// Bounded backend relay helper.
#[derive(Debug, Clone)]
pub struct BackendRelay {
    protocol: MailProtocol,
    limits: Limits,
    phase: RelayPhase,
    relayed_lines: usize,
}

impl BackendRelay {
    /// Create a relay for `protocol` with the session bounds.
    pub fn new(protocol: MailProtocol, limits: &Limits) -> Self {
        Self {
            protocol,
            limits: limits.sanitized(),
            phase: RelayPhase::AwaitGreeting,
            relayed_lines: 0,
        }
    }

    /// Protocol of the relayed session.
    pub fn protocol(&self) -> MailProtocol {
        self.protocol
    }

    /// Current phase.
    pub fn phase(&self) -> RelayPhase {
        self.phase
    }

    /// Validate the backend greeting for the session protocol.
    ///
    /// # Errors
    ///
    /// Fails closed when the line does not match the protocol greeting form
    /// (`+OK`, `* OK`/`* PREAUTH`, `220`) or when called out of order.
    pub fn on_greeting(&mut self, line: &str) -> Result<()> {
        if self.phase != RelayPhase::AwaitGreeting {
            return Err(Error::unexpected(
                self.protocol.as_str(),
                "backend greeting",
            ));
        }
        if line.len() > self.limits.max_line_bytes {
            return Err(Error::LineTooLong {
                protocol: self.protocol.as_str(),
                maximum: self.limits.max_line_bytes,
            });
        }
        let matched = match self.protocol {
            MailProtocol::Pop3 => line.starts_with("+OK"),
            MailProtocol::Imap => line.starts_with("* OK") || line.starts_with("* PREAUTH"),
            MailProtocol::Smtp => line.starts_with("220"),
        };
        if !matched {
            return Err(Error::malformed(
                self.protocol.as_str(),
                "backend greeting",
                "unexpected greeting",
            ));
        }
        self.phase = RelayPhase::Ready;
        Ok(())
    }

    /// Relay one line to the opposite side (CRLF re-applied).
    ///
    /// # Errors
    ///
    /// Fails closed before the greeting, after finish, for an oversized or
    /// control-byte line, or when the relayed-line bound is exhausted.
    pub fn relay_line(&mut self, line: &str) -> Result<Vec<u8>> {
        if self.phase != RelayPhase::Ready {
            return Err(Error::unexpected(
                self.protocol.as_str(),
                "relay line before the backend greeting",
            ));
        }
        if self.relayed_lines >= self.limits.max_multiline_lines {
            return Err(Error::limit(
                self.protocol.as_str(),
                "relayed lines",
                self.limits.max_multiline_lines,
            ));
        }
        let encoded = encode_line(line, &self.limits)?;
        self.relayed_lines = self.relayed_lines.saturating_add(1);
        Ok(encoded)
    }

    /// Number of lines relayed so far.
    pub fn relayed_lines(&self) -> usize {
        self.relayed_lines
    }

    /// Finish the relay phase.
    ///
    /// # Errors
    ///
    /// Fails closed unless the greeting was already accepted.
    pub fn finish(&mut self) -> Result<()> {
        if self.phase != RelayPhase::Ready {
            return Err(Error::unexpected(self.protocol.as_str(), "relay finish"));
        }
        self.phase = RelayPhase::Finished;
        Ok(())
    }
}

/// Bounded backend bootstrap sequence inputs.
#[derive(Debug, Clone, Default)]
pub struct BackendBootstrap {
    /// SMTP `EHLO` name to send before relaying (nginx `proxy_smtp_auth`/xclient
    /// flow). Ignored for POP3/IMAP.
    pub ehlo_name: Option<String>,
    /// SMTP `XCLIENT` request to send after `EHLO`.
    pub xclient: Option<XclientRequest>,
    /// Whether a capability probe should be sent (POP3 `CAPA`; IMAP needs a
    /// caller-allocated tag and is left to [`crate::imap::ImapClient`]).
    pub capability_probe: bool,
}

/// Build the bounded backend bootstrap lines (already CRLF-terminated).
///
/// SMTP: optional `EHLO <name>`, optional `XCLIENT ...`.
/// POP3: optional `CAPA`.
/// IMAP: empty (the caller builds `<tag> CAPABILITY` with
/// [`crate::imap::ImapClient::capability`]).
///
/// # Errors
///
/// Fails closed for invalid/oversized bootstrap values.
pub fn bootstrap_lines(
    protocol: MailProtocol,
    bootstrap: &BackendBootstrap,
    limits: &Limits,
) -> Result<Vec<Vec<u8>>> {
    let limits = limits.sanitized();
    let mut lines = Vec::new();
    match protocol {
        MailProtocol::Smtp => {
            if let Some(ehlo_name) = &bootstrap.ehlo_name {
                if ehlo_name.is_empty()
                    || ehlo_name.len() > 255
                    || ehlo_name
                        .bytes()
                        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n' || byte == b' ')
                {
                    return Err(Error::malformed(
                        "smtp",
                        "EHLO name",
                        "invalid bootstrap name",
                    ));
                }
                lines.push(encode_line(&format!("EHLO {ehlo_name}"), &limits)?);
            }
            if let Some(xclient) = &bootstrap.xclient {
                lines.push(encode_line(&xclient.encode_line(&limits)?, &limits)?);
            }
        }
        MailProtocol::Pop3 => {
            if bootstrap.capability_probe {
                lines.push(encode_line("CAPA", &limits)?);
            }
        }
        MailProtocol::Imap => {}
    }
    if lines.len() > limits.max_pipeline_commands {
        return Err(Error::limit(
            "mail",
            "bootstrap lines",
            limits.max_pipeline_commands,
        ));
    }
    Ok(lines)
}
