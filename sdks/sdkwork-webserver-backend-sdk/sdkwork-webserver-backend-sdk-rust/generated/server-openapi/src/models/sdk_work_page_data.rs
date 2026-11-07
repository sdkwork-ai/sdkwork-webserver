use serde::{Deserialize, Serialize};

use crate::models::{PageInfo};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SdkWorkPageData {
    pub items: Vec<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}
