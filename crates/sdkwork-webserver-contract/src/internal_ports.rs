use async_trait::async_trait;
pub use sdkwork_webserver_core::website_runtime::{
    WebsiteRuntimeSetSnapshot, MAX_WEBSITE_RUNTIME_SET_BYTES,
};
use serde::{Deserialize, Serialize};

use crate::cluster::{
    ClusterDrainCompleteResponse, ClusterHeartbeatRequest, ClusterHeartbeatResponse,
    ClusterPeerDirectoryResponse, ClusterRegistrationRequest, ClusterRegistrationResponse,
    ClusterSyncAckRequest, ClusterSyncManifest, ClusterSyncState,
};
use crate::problem::WebServiceResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedMachineCredential {
    pub tenant_id: i64,
    pub subject_id: String,
    pub app_id: String,
    pub permission_scope: Vec<String>,
}

#[async_trait]
pub trait MachineCredentialAuthenticator: Send + Sync {
    async fn authenticate_machine_credential(
        &self,
        credential: &str,
    ) -> WebServiceResult<Option<AuthenticatedMachineCredential>>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebInternalRequestContext {
    pub tenant_id: i64,
    pub subject_id: String,
    pub agent_node_uuid: Option<String>,
    pub can_publish_cross_tenant: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishRuntimeAssignmentRequest {
    pub runtime_set: WebsiteRuntimeSetSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeAssignment {
    pub assignment_uuid: String,
    pub node_uuid: String,
    pub environment: String,
    pub generation: String,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub assigned_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeAssignmentDelivery {
    pub unchanged: bool,
    pub assignment: RuntimeAssignment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_observation_state: Option<RuntimeObservationState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_set: Option<WebsiteRuntimeSetSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeObservationState {
    Received,
    Validated,
    Staged,
    Active,
    Rejected,
}

impl RuntimeObservationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "RECEIVED",
            Self::Validated => "VALIDATED",
            Self::Staged => "STAGED",
            Self::Active => "ACTIVE",
            Self::Rejected => "REJECTED",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Received => 1,
            Self::Validated => 2,
            Self::Staged => 3,
            Self::Active => 4,
            Self::Rejected => 5,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Active | Self::Rejected)
    }
}

impl TryFrom<&str> for RuntimeObservationState {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "RECEIVED" => Ok(Self::Received),
            "VALIDATED" => Ok(Self::Validated),
            "STAGED" => Ok(Self::Staged),
            "ACTIVE" => Ok(Self::Active),
            "REJECTED" => Ok(Self::Rejected),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRuntimeObservationRequest {
    pub generation: String,
    pub snapshot_sha256: String,
    pub state: RuntimeObservationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeObservation {
    pub observation_uuid: String,
    pub assignment_uuid: String,
    pub tenant_id: String,
    pub node_uuid: String,
    pub environment: String,
    pub generation: String,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub state: RuntimeObservationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub observed_at: String,
}

#[async_trait]
pub trait WebInternalApi: Send + Sync {
    async fn publish_runtime_assignment(
        &self,
        context: &WebInternalRequestContext,
        node_uuid: &str,
        environment: &str,
        request: &PublishRuntimeAssignmentRequest,
    ) -> WebServiceResult<RuntimeAssignment>;

    async fn retrieve_current_runtime_assignment(
        &self,
        context: &WebInternalRequestContext,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery>;

    async fn create_runtime_observation(
        &self,
        context: &WebInternalRequestContext,
        snapshot_uuid: &str,
        request: &CreateRuntimeObservationRequest,
    ) -> WebServiceResult<RuntimeObservation>;

    async fn retrieve_latest_runtime_observation(
        &self,
        context: &WebInternalRequestContext,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation>;

    /// Registers (or re-registers) a webserver process instance and its host
    /// machine in the distributed cluster registry. The request must arrive on
    /// the machine-only surface authenticated with the shared cluster
    /// registration credential (`wreg_` prefix); the response carries the
    /// instance's own heartbeat token (`winst_` prefix).
    async fn register_cluster_instance(
        &self,
        context: &WebInternalRequestContext,
        request: &ClusterRegistrationRequest,
    ) -> WebServiceResult<ClusterRegistrationResponse>;

    /// Records one instance heartbeat: liveness, health state, and resource
    /// metrics. The response refreshes the peer directory and delivers any
    /// pending peer mailbox messages (at-most-once handoff).
    async fn record_cluster_heartbeat(
        &self,
        context: &WebInternalRequestContext,
        request: &ClusterHeartbeatRequest,
    ) -> WebServiceResult<ClusterHeartbeatResponse>;

    /// Returns the current cluster peer directory for the authenticated
    /// instance.
    async fn retrieve_cluster_peers(
        &self,
        context: &WebInternalRequestContext,
    ) -> WebServiceResult<ClusterPeerDirectoryResponse>;

    /// Serves one desired-state sync manifest (config or applications) to
    /// the authenticated instance. The subject id must be the instance
    /// uuid; the manifest carries the cluster's current desired revision
    /// payload for the requested kind.
    async fn retrieve_cluster_sync_manifest(
        &self,
        context: &WebInternalRequestContext,
        kind: &str,
    ) -> WebServiceResult<ClusterSyncManifest>;

    /// Records one node sync acknowledgment (applied revision + status),
    /// updating the instance's drift state.
    async fn record_cluster_sync_ack(
        &self,
        context: &WebInternalRequestContext,
        request: &ClusterSyncAckRequest,
    ) -> WebServiceResult<ClusterSyncState>;

    /// Node acknowledges graceful-drain completion: registry clears the
    /// drain flags and marks the instance stopped.
    async fn record_cluster_drain_complete(
        &self,
        context: &WebInternalRequestContext,
    ) -> WebServiceResult<ClusterDrainCompleteResponse>;
}
