use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHostRef {
    pub id: String,

    pub name: String,

    pub hostname: String,
}
