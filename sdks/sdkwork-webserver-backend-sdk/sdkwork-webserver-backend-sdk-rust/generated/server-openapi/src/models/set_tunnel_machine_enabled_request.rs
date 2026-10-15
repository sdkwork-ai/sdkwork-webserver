use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SetTunnelMachineEnabledRequest {
    pub enabled: bool,
}
