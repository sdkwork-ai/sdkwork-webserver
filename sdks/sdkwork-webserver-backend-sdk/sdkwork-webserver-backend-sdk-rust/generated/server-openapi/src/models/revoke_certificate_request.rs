use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RevokeCertificateRequest {
    /// RFC 5280 section 5.3.1 revocation reason.
    pub reason: String,
}
