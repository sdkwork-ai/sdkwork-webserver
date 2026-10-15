use serde::{Deserialize, Serialize};

use crate::models::{TunnelMachineResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TunnelMachineListResponse {
    pub machines: Vec<TunnelMachineResponse>,
}
