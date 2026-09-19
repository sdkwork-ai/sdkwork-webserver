use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterRef {
    pub id: String,

    pub name: String,

    pub code: String,
}
