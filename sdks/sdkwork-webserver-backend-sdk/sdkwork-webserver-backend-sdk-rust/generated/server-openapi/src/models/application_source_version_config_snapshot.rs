use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationSourceVersionConfigSnapshot {
    #[serde(rename = "appConfigPath")]
    pub app_config_path: String,

    #[serde(rename = "deploymentConfigPath")]
    pub deployment_config_path: String,

    #[serde(rename = "appConfigDetected")]
    pub app_config_detected: bool,

    #[serde(rename = "deploymentConfigDetected")]
    pub deployment_config_detected: bool,
}
