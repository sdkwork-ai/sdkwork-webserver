use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateCertificateRequest {
    #[serde(rename = "autoRenew")]
    pub auto_renew: bool,
}
