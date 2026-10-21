use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainDnsRecordStatusRequest {
    /// `true` resumes the record; `false` pauses it (paused resolution).
    pub enabled: bool,
}
