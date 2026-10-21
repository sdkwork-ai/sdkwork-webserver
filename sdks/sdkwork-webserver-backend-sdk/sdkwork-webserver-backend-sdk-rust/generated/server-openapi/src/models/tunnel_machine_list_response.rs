use serde::{Deserialize, Serialize};

use crate::models::{PageInfo, TunnelMachineResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TunnelMachineListResponse {
    pub machines: Vec<TunnelMachineResponse>,

    #[serde(rename = "pageInfo")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_info: Option<PageInfo>,
}
