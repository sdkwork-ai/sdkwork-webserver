use serde::{Deserialize, Serialize};

use crate::models::{WebserverConfigEntry};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigCatalog {
    /// Web Server runtime configuration root the catalog was enumerated from.
    #[serde(rename = "configRoot")]
    pub config_root: String,

    pub items: Vec<WebserverConfigEntry>,
}
