//! Client-facing `FrontendSession` state machine shared by POP3/IMAP/SMTP.
//!
//! The listener wiring feeds parsed actions into this machine and gets either the
//! next explicit state or a fail-closed transition error. STARTTLS boundaries,
//! implicit TLS, auth completion and the post-auth relay phase are modelled here
//! so every protocol follows the same audited transition table.

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Mail protocol of one frontend listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailProtocol {
    /// POP3.
    Pop3,
    /// IMAP.
    Imap,
    /// SMTP.
    Smtp,
}

impl MailProtocol {
    /// Lowercase protocol name used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pop3 => "pop3",
            Self::Imap => "imap",
            Self::Smtp => "smtp",
        }
    }
}

/// Explicit frontend session states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendState {
    /// Before/while the client greeting is exchanged.
    Initial,
    /// Client is authenticating (protocol dialog or `auth_http` lookup).
    Authenticating,
    /// Client is authenticated and waiting for the backend/relay phase.
    Authenticated,
    /// STARTTLS was negotiated; the caller must complete the TLS handshake.
    TlsUpgrade,
    /// Client↔backend relay is active.
    Relaying,
    /// Session finished.
    Closed,
}

impl FrontendState {
    /// Stable lowercase label used in errors.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Authenticating => "authenticating",
            Self::Authenticated => "authenticated",
            Self::TlsUpgrade => "tls-upgrade",
            Self::Relaying => "relaying",
            Self::Closed => "closed",
        }
    }
}

/// One externally observed frontend action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendAction {
    /// The client started an auth exchange.
    BeginAuth,
    /// The auth exchange succeeded.
    CompleteAuth,
    /// The client requested STARTTLS.
    BeginTls,
    /// The TLS handshake completed.
    TlsEstablished,
    /// The authenticated session starts relaying.
    BeginRelay,
    /// The session ended (QUIT/LOGOUT/close).
    Close,
}

impl FrontendAction {
    /// Stable lowercase label used in errors.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BeginAuth => "begin-auth",
            Self::CompleteAuth => "complete-auth",
            Self::BeginTls => "begin-tls",
            Self::TlsEstablished => "tls-established",
            Self::BeginRelay => "begin-relay",
            Self::Close => "close",
        }
    }
}

/// Bounded frontend session state machine.
#[derive(Debug, Clone)]
pub struct FrontendSession {
    protocol: MailProtocol,
    state: FrontendState,
    tls_active: bool,
    authenticated: Option<String>,
    actions: usize,
}

impl FrontendSession {
    /// Create a session.
    ///
    /// `implicit_tls` marks a listener that already terminated TLS before the
    /// greeting (nginx `ssl on` mail listener); STARTTLS is then unavailable.
    pub fn new(protocol: MailProtocol, implicit_tls: bool) -> Self {
        Self {
            protocol,
            state: FrontendState::Initial,
            tls_active: implicit_tls,
            authenticated: None,
            actions: 0,
        }
    }

    /// Current state.
    pub fn state(&self) -> FrontendState {
        self.state
    }

    /// Protocol of the session.
    pub fn protocol(&self) -> MailProtocol {
        self.protocol
    }

    /// Whether TLS is active (implicit or after STARTTLS).
    pub fn tls_active(&self) -> bool {
        self.tls_active
    }

    /// Authenticated login, when the session reached it.
    pub fn authenticated_user(&self) -> Option<&str> {
        self.authenticated.as_deref()
    }

    /// Apply one action and return the resulting state.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidTransition`] for any transition outside the
    /// audited table (including a second STARTTLS after TLS is active and any
    /// action after close).
    pub fn apply(
        &mut self,
        action: FrontendAction,
        user: Option<&str>,
        limits: &Limits,
    ) -> Result<FrontendState> {
        let limits = limits.sanitized();
        let next = match (self.state, action) {
            (FrontendState::Initial, FrontendAction::BeginAuth) => FrontendState::Authenticating,
            (FrontendState::Initial, FrontendAction::BeginTls) if !self.tls_active => {
                FrontendState::TlsUpgrade
            }
            (FrontendState::Initial | FrontendState::Authenticating, FrontendAction::Close) => {
                FrontendState::Closed
            }
            (FrontendState::Authenticating, FrontendAction::CompleteAuth) => {
                let Some(user) = user else {
                    return Err(Error::unexpected(
                        self.protocol.as_str(),
                        "CompleteAuth without a user",
                    ));
                };
                if user.is_empty() || user.len() > limits.max_credential_bytes {
                    return Err(Error::limit(
                        self.protocol.as_str(),
                        "credential",
                        limits.max_credential_bytes,
                    ));
                }
                if user
                    .bytes()
                    .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
                {
                    return Err(Error::CredentialByte);
                }
                self.authenticated = Some(user.to_owned());
                FrontendState::Authenticated
            }
            (FrontendState::Authenticating, FrontendAction::BeginTls) if !self.tls_active => {
                FrontendState::TlsUpgrade
            }
            (FrontendState::TlsUpgrade, FrontendAction::TlsEstablished) => {
                self.tls_active = true;
                FrontendState::Authenticating
            }
            (FrontendState::Authenticated, FrontendAction::BeginRelay) => FrontendState::Relaying,
            (FrontendState::Authenticated | FrontendState::Relaying, FrontendAction::Close) => {
                FrontendState::Closed
            }
            (_, FrontendAction::BeginTls) if self.tls_active => {
                return Err(Error::transition(
                    self.protocol.as_str(),
                    self.state.as_str(),
                    FrontendAction::BeginTls.as_str(),
                ));
            }
            _ => {
                return Err(Error::transition(
                    self.protocol.as_str(),
                    self.state.as_str(),
                    action.as_str(),
                ))
            }
        };
        self.state = next;
        self.actions = self.actions.saturating_add(1);
        Ok(next)
    }

    /// Number of accepted transitions.
    pub fn actions(&self) -> usize {
        self.actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits::default()
    }

    #[test]
    fn auth_then_relay_is_the_happy_path_for_every_protocol() {
        for protocol in [MailProtocol::Pop3, MailProtocol::Imap, MailProtocol::Smtp] {
            let mut session = FrontendSession::new(protocol, false);
            assert_eq!(
                session
                    .apply(FrontendAction::BeginAuth, None, &limits())
                    .expect("auth"),
                FrontendState::Authenticating
            );
            assert_eq!(
                session
                    .apply(FrontendAction::CompleteAuth, Some("alice"), &limits())
                    .expect("complete"),
                FrontendState::Authenticated
            );
            assert_eq!(
                session
                    .apply(FrontendAction::BeginRelay, None, &limits())
                    .expect("relay"),
                FrontendState::Relaying
            );
            assert_eq!(session.authenticated_user(), Some("alice"));
            assert_eq!(
                session
                    .apply(FrontendAction::Close, None, &limits())
                    .expect("close"),
                FrontendState::Closed
            );
        }
    }

    #[test]
    fn starttls_boundary_is_explicit_and_single_use() {
        let mut session = FrontendSession::new(MailProtocol::Imap, false);
        assert_eq!(
            session
                .apply(FrontendAction::BeginAuth, None, &limits())
                .expect("auth"),
            FrontendState::Authenticating
        );
        assert_eq!(
            session
                .apply(FrontendAction::BeginTls, None, &limits())
                .expect("tls"),
            FrontendState::TlsUpgrade
        );
        assert_eq!(
            session
                .apply(FrontendAction::TlsEstablished, None, &limits())
                .expect("established"),
            FrontendState::Authenticating
        );
        assert!(session.tls_active());
        assert!(matches!(
            session.apply(FrontendAction::BeginTls, None, &limits()),
            Err(Error::InvalidTransition { .. })
        ));
    }

    #[test]
    fn implicit_tls_disallows_starttls() {
        let mut session = FrontendSession::new(MailProtocol::Pop3, true);
        assert!(session.tls_active());
        assert!(matches!(
            session.apply(FrontendAction::BeginTls, None, &limits()),
            Err(Error::InvalidTransition { .. })
        ));
    }

    #[test]
    fn invalid_transitions_and_actions_after_close_fail_closed() {
        let mut session = FrontendSession::new(MailProtocol::Smtp, false);
        assert!(session
            .apply(FrontendAction::CompleteAuth, Some("alice"), &limits())
            .is_err());
        assert!(session
            .apply(FrontendAction::BeginRelay, None, &limits())
            .is_err());
        session
            .apply(FrontendAction::Close, None, &limits())
            .expect("close");
        assert!(session
            .apply(FrontendAction::Close, None, &limits())
            .is_err());
    }
}
