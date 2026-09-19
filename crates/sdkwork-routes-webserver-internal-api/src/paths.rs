pub const PREFIX: &str = "/internal/v3/api";

pub const RUNTIME_ASSIGNMENT: &str =
    "/internal/v3/api/web/runtime_assignments/{nodeUuid}/{environment}";
pub const CURRENT_RUNTIME_ASSIGNMENT: &str = "/internal/v3/api/web/runtime_assignments/current";
pub const RUNTIME_OBSERVATIONS: &str =
    "/internal/v3/api/web/runtime_assignments/{snapshotUuid}/observations";
pub const LATEST_RUNTIME_OBSERVATION: &str =
    "/internal/v3/api/web/runtime_assignments/{snapshotUuid}/observations/latest";

pub const CLUSTER_REGISTER: &str = "/internal/v3/api/web/cluster/instances/register";
pub const CLUSTER_HEARTBEAT: &str = "/internal/v3/api/web/cluster/instances/heartbeat";
pub const CLUSTER_PEERS: &str = "/internal/v3/api/web/cluster/peers";
