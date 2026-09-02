use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentCertificateBundle {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,

    #[serde(rename = "certName")]
    pub cert_name: String,

    pub fingerprint: String,

    pub hostnames: Vec<String>,

    #[serde(rename = "fullchainPem")]
    pub fullchain_pem: String,

    #[serde(rename = "privkeyPem")]
    pub privkey_pem: String,
}
