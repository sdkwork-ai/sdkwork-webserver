use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MediaChecksum {
    pub algorithm: String,

    pub value: String,
}
