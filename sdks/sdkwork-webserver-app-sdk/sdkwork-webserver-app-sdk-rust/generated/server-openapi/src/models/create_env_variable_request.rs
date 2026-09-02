use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateEnvVariableRequest {
    pub key: String,

    pub value: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,

    #[serde(rename = "isSecret")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_secret: Option<bool>,
}
