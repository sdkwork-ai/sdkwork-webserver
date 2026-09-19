use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterInstanceRef {
    pub id: String,

    pub name: String,

    pub role: String,
}
