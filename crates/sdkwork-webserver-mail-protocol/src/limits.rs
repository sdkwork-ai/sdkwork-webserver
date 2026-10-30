//! Bounds for every adjustable buffer in the mail-protocol substrate.

/// Per-session protocol bounds.
///
/// Every field is a hard maximum: a codec fails closed with
/// [`crate::Error::LineTooLong`], [`crate::Error::LimitExceeded`] or
/// [`crate::Error::LiteralTooLarge`] instead of growing a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes of one protocol line, excluding CRLF.
    pub max_line_bytes: usize,
    /// Maximum bytes of an auth (base64) line.
    pub max_auth_line_bytes: usize,
    /// Maximum decoded credential length.
    pub max_credential_bytes: usize,
    /// Maximum IMAP literal payload size.
    pub max_literal_bytes: usize,
    /// Maximum lines in one dot-stuffed multiline response.
    pub max_multiline_lines: usize,
    /// Maximum total bytes of one dot-stuffed multiline response.
    pub max_multiline_bytes: usize,
    /// Maximum number of advertised capabilities kept per session.
    pub max_capabilities: usize,
    /// Maximum attributes in one `XCLIENT` line.
    pub max_xclient_attributes: usize,
    /// Maximum bytes of an `XCLIENT`/PROXY header line.
    pub max_relay_header_bytes: usize,
    /// Maximum SMTP `DATA` payload bytes.
    pub max_smtp_data_bytes: usize,
    /// Maximum commands coalesced into one SMTP pipeline.
    pub max_pipeline_commands: usize,
}

impl Limits {
    /// Neutral, nginx-scale defaults for one mail session.
    ///
    /// `max_line_bytes` covers `AUTH`/`MAIL FROM`/tagged IMAP commands;
    /// `max_multiline_bytes` bounds a `CAPA`/`CAPABILITY`/`DATA` response;
    /// `max_literal_bytes` bounds one IMAP literal.
    pub fn defaults() -> Self {
        Self {
            max_line_bytes: 4096,
            max_auth_line_bytes: 8192,
            max_credential_bytes: 1024,
            max_literal_bytes: 256 * 1024,
            max_multiline_lines: 1024,
            max_multiline_bytes: 512 * 1024,
            max_capabilities: 64,
            max_xclient_attributes: 16,
            max_relay_header_bytes: 512,
            max_smtp_data_bytes: 10 * 1024 * 1024,
            max_pipeline_commands: 16,
        }
    }

    /// Replace zero bounds with `1` so a sanitized copy can never disable a limit.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.max_line_bytes = self.max_line_bytes.max(1);
        self.max_auth_line_bytes = self.max_auth_line_bytes.max(1);
        self.max_credential_bytes = self.max_credential_bytes.max(1);
        self.max_literal_bytes = self.max_literal_bytes.max(1);
        self.max_multiline_lines = self.max_multiline_lines.max(1);
        self.max_multiline_bytes = self.max_multiline_bytes.max(1);
        self.max_capabilities = self.max_capabilities.max(1);
        self.max_xclient_attributes = self.max_xclient_attributes.max(1);
        self.max_relay_header_bytes = self.max_relay_header_bytes.max(1);
        self.max_smtp_data_bytes = self.max_smtp_data_bytes.max(1);
        self.max_pipeline_commands = self.max_pipeline_commands.max(1);
        self
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_never_disables_a_limit() {
        let limits = Limits {
            max_line_bytes: 0,
            max_auth_line_bytes: 0,
            max_credential_bytes: 0,
            max_literal_bytes: 0,
            max_multiline_lines: 0,
            max_multiline_bytes: 0,
            max_capabilities: 0,
            max_xclient_attributes: 0,
            max_relay_header_bytes: 0,
            max_smtp_data_bytes: 0,
            max_pipeline_commands: 0,
        }
        .sanitized();
        assert_eq!(limits.max_line_bytes, 1);
        assert_eq!(limits.max_literal_bytes, 1);
        assert_eq!(limits.max_pipeline_commands, 1);
    }
}
