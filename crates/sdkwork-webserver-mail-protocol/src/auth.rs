//! SASL PLAIN/LOGIN auth-line codecs and the LOGIN dialog.
//!
//! nginx's mail proxy asks the backend with `AUTH PLAIN`/`AUTH LOGIN` (POP3,
//! IMAP) or `AUTH PLAIN`/`AUTH LOGIN` (SMTP), so the substrate needs exact,
//! bounded encoders for both directions and a small state machine for the
//! two-prompt LOGIN exchange.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::error::{Error, Result};
use crate::limits::Limits;

/// Supported SASL mechanisms of the bounded mail frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMechanism {
    /// RFC 4616 PLAIN (`authzid NUL authcid NUL passwd`).
    Plain,
    /// The prompt-driven LOGIN mechanism.
    Login,
}

impl AuthMechanism {
    /// Parse an `AUTH`/`AUTHENTICATE` mechanism token (ASCII case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedMechanism`] for any other mechanism.
    pub fn from_token(protocol: &'static str, token: &str) -> Result<Self> {
        if token.eq_ignore_ascii_case("PLAIN") {
            Ok(Self::Plain)
        } else if token.eq_ignore_ascii_case("LOGIN") {
            Ok(Self::Login)
        } else if token.len() > 64 {
            Err(Error::limit("auth", "mechanism", 64))
        } else {
            Err(Error::UnsupportedMechanism {
                protocol,
                mechanism: token.to_owned(),
            })
        }
    }

    /// Wire token (`PLAIN` / `LOGIN`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::Login => "LOGIN",
        }
    }
}

/// Decoded PLAIN credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainCredentials {
    /// Authorization identity (may be empty).
    pub authorization_id: String,
    /// Authentication identity (login).
    pub authentication_id: String,
    /// Password.
    pub password: String,
}

/// Validate one credential component: no NUL/CR/LF and bounded length.
fn validate_component(component: &str, limits: &Limits) -> Result<()> {
    let limits = limits.sanitized();
    if component.len() > limits.max_credential_bytes {
        return Err(Error::limit(
            "auth",
            "credential",
            limits.max_credential_bytes,
        ));
    }
    if component
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
    {
        return Err(Error::CredentialByte);
    }
    Ok(())
}

/// Encode an `AUTH PLAIN` initial response: base64 of `NUL user NUL pass`.
///
/// # Errors
///
/// Returns [`Error::CredentialByte`] for a NUL/CR/LF component and a limit error
/// for an oversized credential component.
pub fn encode_plain(user: &str, password: &str, limits: &Limits) -> Result<String> {
    validate_component(user, limits)?;
    validate_component(password, limits)?;
    let mut blob = Vec::with_capacity(user.len() + password.len() + 2);
    blob.push(0);
    blob.extend_from_slice(user.as_bytes());
    blob.push(0);
    blob.extend_from_slice(password.as_bytes());
    Ok(STANDARD.encode(blob))
}

/// Decode an `AUTH PLAIN` response into credentials.
///
/// # Errors
///
/// Fails closed on invalid base64, a blob that is not three NUL-separated
/// components, non-UTF-8 content, or an oversized credential.
pub fn decode_plain(input: &str, limits: &Limits) -> Result<PlainCredentials> {
    let limits = limits.sanitized();
    if input.len() > limits.max_auth_line_bytes {
        return Err(Error::limit(
            "auth",
            "auth line",
            limits.max_auth_line_bytes,
        ));
    }
    let decoded = STANDARD.decode(input.trim()).map_err(|_| Error::Base64)?;
    if decoded.len() > limits.max_credential_bytes.saturating_mul(3) {
        return Err(Error::limit(
            "auth",
            "credential",
            limits.max_credential_bytes,
        ));
    }
    let mut parts = decoded.split(|byte| *byte == 0);
    let authorization_id = parts.next().ok_or(Error::PlainCredentials)?;
    let authentication_id = parts.next().ok_or(Error::PlainCredentials)?;
    let password = parts.next().ok_or(Error::PlainCredentials)?;
    if parts.next().is_some() {
        return Err(Error::PlainCredentials);
    }
    let decode = |bytes: &[u8], field: &'static str| {
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8 {
            protocol: "auth",
            field,
        })
    };
    let credentials = PlainCredentials {
        authorization_id: decode(authorization_id, "authorization id")?,
        authentication_id: decode(authentication_id, "authentication id")?,
        password: decode(password, "password")?,
    };
    validate_component(&credentials.authentication_id, &limits)?;
    validate_component(&credentials.password, &limits)?;
    Ok(credentials)
}

/// Encode one LOGIN continuation response (base64 of the component).
///
/// # Errors
///
/// Returns [`Error::CredentialByte`] for forbidden bytes and a limit error for
/// an oversized line.
pub fn encode_login_response(component: &str, limits: &Limits) -> Result<String> {
    validate_component(component, limits)?;
    Ok(STANDARD.encode(component.as_bytes()))
}

/// Decode one LOGIN continuation response.
///
/// # Errors
///
/// Fails closed on invalid base64, non-UTF-8 content, CR/LF/NUL, or an oversized
/// component.
pub fn decode_login_response(input: &str, limits: &Limits) -> Result<String> {
    let limits = limits.sanitized();
    if input.len() > limits.max_auth_line_bytes {
        return Err(Error::limit(
            "auth",
            "auth line",
            limits.max_auth_line_bytes,
        ));
    }
    let decoded = STANDARD.decode(input.trim()).map_err(|_| Error::Base64)?;
    let component = String::from_utf8(decoded).map_err(|_| Error::InvalidUtf8 {
        protocol: "auth",
        field: "login response",
    })?;
    validate_component(&component, &limits)?;
    Ok(component)
}

/// Stage of the two-prompt LOGIN exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStage {
    /// Waiting for the server's username prompt.
    AwaitUsernamePrompt,
    /// Waiting for the server's password prompt.
    AwaitPasswordPrompt,
    /// Both responses were produced.
    Complete,
}

/// Two-prompt LOGIN dialog: nginx reads two server continuation prompts and
/// responds with base64 username then base64 password.
#[derive(Debug, Clone)]
pub struct LoginDialog {
    username_response: String,
    password_response: String,
    stage: LoginStage,
}

impl LoginDialog {
    /// Build the dialog; the credentials are validated and pre-encoded once.
    ///
    /// # Errors
    ///
    /// Fails closed for forbidden credential bytes or oversized components.
    pub fn new(user: &str, password: &str, limits: &Limits) -> Result<Self> {
        Ok(Self {
            username_response: encode_login_response(user, limits)?,
            password_response: encode_login_response(password, limits)?,
            stage: LoginStage::AwaitUsernamePrompt,
        })
    }

    /// Current stage.
    pub fn stage(&self) -> LoginStage {
        self.stage
    }

    /// Consume one server continuation prompt and return the next response line,
    /// or `None` once the dialog is complete.
    ///
    /// # Errors
    ///
    /// Fails closed when a prompt arrives after completion.
    pub fn on_prompt(&mut self) -> Result<Option<&str>> {
        match self.stage {
            LoginStage::AwaitUsernamePrompt => {
                self.stage = LoginStage::AwaitPasswordPrompt;
                Ok(Some(&self.username_response))
            }
            LoginStage::AwaitPasswordPrompt => {
                self.stage = LoginStage::Complete;
                Ok(Some(&self.password_response))
            }
            LoginStage::Complete => Err(Error::unexpected("auth", "LOGIN prompt after completion")),
        }
    }

    /// Whether both responses were produced.
    pub fn is_complete(&self) -> bool {
        self.stage == LoginStage::Complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_round_trips_and_rejects_malformed_blobs() {
        let limits = Limits::default();
        let encoded = encode_plain("user", "p a s s", &limits).expect("encode");
        let credentials = decode_plain(&encoded, &limits).expect("decode");
        assert_eq!(credentials.authentication_id, "user");
        assert_eq!(credentials.password, "p a s s");
        assert_eq!(credentials.authorization_id, "");
        assert!(matches!(
            decode_plain("not base64 !!", &limits),
            Err(Error::Base64)
        ));
        assert!(matches!(
            decode_plain(&STANDARD.encode(b"NUL-only"), &limits),
            Err(Error::PlainCredentials)
        ));
        assert!(matches!(
            decode_plain(&STANDARD.encode(b"a\0b\0c\0d"), &limits),
            Err(Error::PlainCredentials)
        ));
    }

    #[test]
    fn login_dialog_emits_two_responses_then_fails_closed() {
        let limits = Limits::default();
        let mut dialog = LoginDialog::new("user", "pass", &limits).expect("dialog");
        let user_line = dialog
            .on_prompt()
            .expect("prompt")
            .expect("line")
            .to_owned();
        assert_eq!(
            decode_login_response(&user_line, &limits).expect("user"),
            "user"
        );
        let pass_line = dialog
            .on_prompt()
            .expect("prompt")
            .expect("line")
            .to_owned();
        assert_eq!(
            decode_login_response(&pass_line, &limits).expect("pass"),
            "pass"
        );
        assert!(dialog.is_complete());
        assert!(matches!(dialog.on_prompt(), Err(Error::Unexpected { .. })));
    }

    #[test]
    fn credential_bytes_are_validated() {
        let limits = Limits::default();
        assert!(encode_plain("u\r\n", "p", &limits).is_err());
        assert!(encode_plain("u", "p\0", &limits).is_err());
        let oversized = Limits {
            max_credential_bytes: 2,
            ..Limits::default()
        };
        assert!(encode_plain("long-user", "p", &oversized).is_err());
    }

    #[test]
    fn mechanism_tokens_are_bounded_and_known() {
        assert_eq!(
            AuthMechanism::from_token("smtp", "plain").expect("plain"),
            AuthMechanism::Plain
        );
        assert!(matches!(
            AuthMechanism::from_token("imap", "XOAUTH2"),
            Err(Error::UnsupportedMechanism { .. })
        ));
    }
}
