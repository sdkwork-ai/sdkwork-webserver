use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AuditLogResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Operator user id as a string to avoid JavaScript precision loss.
    #[serde(rename = "operatorId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<String>,

    #[serde(rename = "operatorType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    #[serde(rename = "targetType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,

    /// Target snowflake id as a string to avoid JavaScript precision loss. Null when the audit action has no specific numeric target.
    #[serde(rename = "targetId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,

    #[serde(rename = "targetUuid")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_uuid: Option<String>,

    #[serde(rename = "ipAddress")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "createdAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
