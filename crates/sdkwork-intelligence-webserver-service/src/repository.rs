//! Repository port consumed by the Web service layer.

use async_trait::async_trait;
use sdkwork_webserver_contract::WebServiceResult;
use sdkwork_webserver_contract::{
    AgentHeartbeatRequest, AgentHeartbeatResponse, AgentSyncResponse, ApplicationPage,
    ApplicationResponse, AuditLogPage, CertificateDistributionPage, CertificateIssueUpdate,
    CertificateOperationAcceptedResponse, CertificateOperationLease, CertificateOperationResponse,
    CertificatePage, CertificateResponse, ClusterEventPage, ClusterHeartbeatSamplePage,
    ClusterHostPage, ClusterHostResponse, ClusterInstancePage, ClusterInstanceResponse,
    ClusterOverviewResponse, ClusterPage, ClusterPeer, ClusterPeerMessage, ClusterResponse,
    CreateApplicationRequest, CreateClusterRequest, CreateDeploymentRequest, CreateDomainRequest,
    CreateEnvVariableRequest, CreateHealthCheckRequest, CreateListenerCertificateBindingRequest,
    CreateManagedDomainRequest, CreateNginxConfigRequest, CreatePlatformTargetRequest,
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, CreateServerRequest,
    CreateServerResponse, CreateSourceVersionRequest, DeploymentPage, DeploymentResponse,
    DomainPage, DomainResponse, EnvVariablePage, EnvVariableResponse, HealthCheckPage,
    HealthCheckResponse, IssueCertificateRequest, ListApplicationsQuery, ListAuditLogsQuery,
    ListNginxConfigsQuery, ListRootDomainsQuery, ListenerCertificateBindingPage,
    ListenerCertificateBindingResponse, NginxConfigPage, NginxConfigResponse, NginxStatusResponse,
    PlatformTargetPage, PlatformTargetResponse, RevokeCertificateRequest, RootDomainPage,
    RootDomainResponse, RuntimeAssignment, RuntimeAssignmentDelivery, RuntimeObservation,
    RuntimeObservationState, ServerPage, SourceVersionPage, SourceVersionResponse,
    TlsCertificateAssignmentMaterial, UpdateApplicationRequest, UpdateClusterHostRequest,
    UpdateClusterInstanceRequest, UpdateClusterRequest, UpdateDomainApplicationBindingRequest,
    UpdateEnvVariableRequest, UpdateNginxConfigRequest, UpdateRootDomainRequest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAssignmentTarget {
    pub server_id: i64,
    pub node_uuid: String,
    pub tenant_id: i64,
    pub tenant_scope_hash: String,
}

#[derive(Clone, Debug)]
pub struct CertificateRevocationMaterial {
    pub cert_type: i32,
    pub fullchain_pem: String,
}

#[derive(Clone, Debug)]
pub struct RuntimeAssignmentWrite {
    pub tenant_id: i64,
    pub server_id: i64,
    pub node_uuid: String,
    pub environment: String,
    pub generation: u64,
    pub snapshot_uuid: String,
    pub snapshot_sha256: String,
    pub runtime_set_json: String,
    pub runtime_set_bytes: usize,
    pub assigned_by_subject: String,
}

#[derive(Clone, Debug)]
pub struct RuntimeObservationWrite {
    pub tenant_id: i64,
    pub node_uuid: String,
    pub snapshot_uuid: String,
    pub generation: u64,
    pub snapshot_sha256: String,
    pub state: RuntimeObservationState,
    pub node_version: Option<String>,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationChallenge {
    /// Owning tenant. Carried on the challenge because the automatic
    /// verification sweep is cross-tenant: it selects due challenges for the
    /// whole deployment and then records each observation under its own tenant.
    pub tenant_id: i64,
    pub challenge_id: String,
    pub hostname: String,
    pub method: String,
    pub record_name: String,
    pub proof_sha256: String,
    pub status: String,
    pub attempt_count: i32,
    pub expires_at: String,
    pub next_attempt_at: Option<String>,
    pub checked_at: Option<String>,
    pub failure_code: Option<String>,
    pub ready_for_check: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainVerificationObservation {
    pub observed_sha256: Option<String>,
    pub failure_code: Option<String>,
}

/// Host identity write used by cluster registration (system basics, remote
/// IP, local IPs, machine code, MAC addresses).
#[derive(Clone, Debug)]
pub struct ClusterHostUpsert {
    pub tenant_id: i64,
    pub cluster_id: i64,
    pub name: String,
    pub hostname: String,
    pub machine_code: String,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: Option<String>,
    pub cpu_model: Option<String>,
    pub cpu_cores: Option<i32>,
    pub memory_total_mb: Option<i64>,
    pub remote_ip: Option<String>,
    pub local_ips: Vec<String>,
    pub mac_addresses: Vec<String>,
    pub daemon_version: Option<String>,
    /// Join mode: `0` = LAN (same-subnet), `1` = TUNNEL (API-only through
    /// the reverse tunnel).
    pub join_mode: i32,
    /// Tunnel route domain reaching this host (TUNNEL hosts only).
    pub tunnel_route_domain: Option<String>,
    /// Advertised gateway endpoint this host dials (TUNNEL hosts only).
    pub tunnel_endpoint: Option<String>,
}

/// Process identity write used by cluster registration (one row per PID per
/// host). `instance_token_hash` is merged into the instance metadata so the
/// heartbeat token authenticates after registration.
#[derive(Clone, Debug)]
pub struct ClusterInstanceUpsert {
    pub tenant_id: i64,
    pub host_id: i64,
    pub cluster_id: i64,
    pub name: String,
    pub role: String,
    pub environment: String,
    pub process_pid: i32,
    pub process_started_at: String,
    pub bind_host: Option<String>,
    pub bind_port: Option<i32>,
    pub public_endpoint: Option<String>,
    pub build_version: Option<String>,
    pub instance_token_hash: String,
    /// Join mode: `0` = LAN (same-subnet), `1` = TUNNEL (API-only through
    /// the reverse tunnel).
    pub join_mode: i32,
    /// Tunnel route domain reaching this instance (TUNNEL instances).
    pub tunnel_route_domain: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ClusterHeartbeatWrite {
    pub tenant_id: i64,
    pub instance_id: i64,
    pub host_id: i64,
    pub status: i32,
    pub health_state: String,
    pub uptime_seconds: i64,
    pub build_version: Option<String>,
    pub metrics_json: String,
    pub reported_at: String,
    /// Node service quality score 0..=100 computed from the heartbeat
    /// quality sample (`None` when no sample was reported).
    pub quality_score: Option<i32>,
}

/// Desired-state revision publication (cluster admin action): registers one
/// revision for a sync kind and marks it as the desired state for every
/// instance of the cluster.
#[derive(Clone, Debug)]
pub struct ClusterSyncRevisionPublish {
    pub tenant_id: i64,
    pub cluster_id: i64,
    /// Sync kind: `0` = config, `1` = applications.
    pub kind: i32,
    /// Monotonic revision label (snowflake id string).
    pub revision: String,
    /// SHA-256 of the canonical payload JSON.
    pub sha256: String,
    pub payload: serde_json::Value,
    pub size_bytes: i64,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Latest desired revision for one sync kind (the instance-facing contract).
#[derive(Clone, Debug)]
pub struct ClusterSyncDesired {
    pub kind: i32,
    pub revision: String,
}

/// Payload of one stored sync revision (node fetch on drift detection).
#[derive(Clone, Debug)]
pub struct ClusterSyncRevisionPayload {
    pub kind: i32,
    pub revision: String,
    pub sha256: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

/// Per-instance sync acknowledgment write: records the applied revision for
/// one kind and recomputes the aggregate sync status.
#[derive(Clone, Debug)]
pub struct ClusterSyncAckWrite {
    pub tenant_id: i64,
    /// Instance internal id.
    pub instance_id: i64,
    /// Sync kind: `0` = config, `1` = applications.
    pub kind: i32,
    /// Revision the instance applied.
    pub applied_revision: String,
    /// Ack status: `1` = applied (in sync), `3` = failed.
    pub status: i32,
    pub updated_at: String,
}

/// Auto-discovery read model: everything the routing topology builder needs
/// for one cluster, sourced from the registry (single source of truth).
#[derive(Clone, Debug)]
pub struct ClusterRoutingDiscovery {
    pub cluster_uuid: String,
    /// Configured strategy label (`round_robin` default).
    pub lb_strategy: String,
    /// Service domains the cluster serves (auto-routing match keys).
    pub served_domains: Vec<String>,
    /// Routeable instances (online, cordoned/draining/ejected excluded).
    pub instances: Vec<ClusterRoutingInstance>,
}

/// One routeable instance of the discovered cluster.
#[derive(Clone, Debug)]
pub struct ClusterRoutingInstance {
    /// Instance uuid (topology identity).
    pub uuid: String,
    /// Internal east-west endpoint (`host:port`, bind-derived).
    pub endpoint: String,
    /// Per-instance LB weight override (1..=10000).
    pub weight: i32,
    pub quality_score: Option<i32>,
}

/// One active health-probe outcome write (auto-eject / auto-recover driver).
#[derive(Clone, Debug)]
pub struct ClusterProbeWrite {
    pub tenant_id: i64,
    pub instance_id: i64,
    /// Whether the probe considered the instance healthy.
    pub healthy: bool,
}

/// Result of recording one probe outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterProbeOutcome {
    /// Consecutive failure count after this probe.
    pub failures: i32,
    /// The instance is ejected after this probe.
    pub ejected: bool,
    /// The eject transition happened on this probe (event emission edge).
    pub eject_transition: bool,
    /// A previously ejected instance recovered on this probe.
    pub recovered: bool,
    /// Instance status before the probe.
    pub was_online: bool,
}

/// Cluster lifecycle event write addressed by uuids; the store resolves the
/// internal foreign keys so service callers never juggle two id planes.
#[derive(Clone, Debug)]
pub struct ClusterEventWrite<'a> {
    pub tenant_id: i64,
    pub cluster_uuid: &'a str,
    pub host_uuid: Option<&'a str>,
    pub instance_uuid: Option<&'a str>,
    pub event_type: &'a str,
    pub severity: &'a str,
    pub message: &'a str,
    pub detail_json: &'a str,
    pub occurred_at: &'a str,
}

/// Peer message enqueue: one row per target member (broadcast materializes a
/// copy per online member at enqueue time).
#[derive(Clone, Debug)]
pub struct ClusterPeerMessageEnqueue<'a> {
    pub tenant_id: i64,
    pub cluster_uuid: &'a str,
    pub from_instance_uuid: Option<&'a str>,
    pub to_instance_uuid: Option<&'a str>,
    pub message_type: &'a str,
    pub payload_json: &'a str,
    pub deliver_at: &'a str,
    pub expires_at: &'a str,
}

/// Resolved registration target cluster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterIdentity {
    pub cluster_id: i64,
    pub cluster_uuid: String,
    pub name: String,
    pub code: String,
    pub heartbeat_interval_seconds: i32,
    pub offline_threshold_seconds: i32,
}

/// Result of an idempotent host/instance registration upsert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterUpsert {
    pub id: i64,
    pub uuid: String,
    pub created: bool,
}

/// Instance credentials resolved by heartbeat token or subject uuid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterInstanceCredentials {
    pub instance_id: i64,
    pub instance_uuid: String,
    pub host_id: i64,
    pub host_uuid: String,
    pub cluster_id: i64,
    pub cluster_uuid: String,
    pub tenant_id: i64,
    pub heartbeat_interval_seconds: i32,
    pub offline_threshold_seconds: i32,
    /// Desired configuration revision (sync plane); `None` when unpublished.
    pub desired_config_revision: Option<String>,
    /// Applied configuration revision (last acknowledged).
    pub applied_config_revision: Option<String>,
    /// Desired applications-manifest revision (sync plane).
    pub desired_applications_revision: Option<String>,
    /// Applied applications-manifest revision (last acknowledged).
    pub applied_applications_revision: Option<String>,
    /// Aggregate sync status integer.
    pub sync_status: i32,
    /// Cordon switch: routed traffic admitted.
    pub routing_enabled: bool,
    /// Graceful drain in progress.
    pub draining: bool,
}

/// Instance status values, mirroring the authored
/// `ClusterInstanceResponse.status` dictionary and the DDL comment on
/// `webserver_cluster_instance.status`
/// (`0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance`).
pub const CLUSTER_INSTANCE_STATUS_OFFLINE: i32 = 0;
pub const CLUSTER_INSTANCE_STATUS_ONLINE: i32 = 1;
pub const CLUSTER_INSTANCE_STATUS_ERROR: i32 = 4;
pub const CLUSTER_INSTANCE_STATUS_MAINTENANCE: i32 = 5;

/// The status a **machine** writer actually stores, given the status already on
/// the row and the status that writer proposes.
///
/// `status` is written by three parties that disagree by design:
///
/// * the **node** reports its own liveness, and its reporter always claims
///   `online` while the process runs (`cluster_self_report` sends a fixed
///   `status: 1`), so registration and every heartbeat propose `1`;
/// * the **active prober** proposes `error` when it ejects a failing instance;
/// * the **platform operator** declares `maintenance` through the instance
///   update endpoint, and maintenance is nothing but an exclusion: the routing
///   pool admits `status = 1` only.
///
/// Because the node's claim arrives on a timer (15s by default), a plain
/// overwrite made an operator's maintenance mark survive at most one heartbeat
/// interval, and the transition recorder even logged a phantom
/// `INSTANCE_ONLINE` right after the mark. Maintenance is therefore the one
/// status no machine writer may clear.
///
/// The operator's own write does **not** come through here — the update path
/// stores what the operator asked for verbatim, which is what lets "End
/// maintenance" end it. Every other machine value keeps the previous
/// last-writer-wins behaviour, including the node's own `drain-complete`
/// report of `offline` (a stopped process is a stronger fact than the mark).
pub fn cluster_operator_owned_status(previous_status: i32, machine_proposed_status: i32) -> i32 {
    if previous_status == CLUSTER_INSTANCE_STATUS_MAINTENANCE
        && machine_proposed_status != CLUSTER_INSTANCE_STATUS_MAINTENANCE
    {
        previous_status
    } else {
        machine_proposed_status
    }
}

/// Previous instance state observed during a heartbeat, plus the status the
/// heartbeat actually recorded, used for lifecycle transition events.
///
/// `status` is the recorded value, not the reported one: the two differ while
/// an operator maintenance mark outranks the node's `online` claim, and
/// lifecycle events must describe what the registry did, not what the node
/// asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterHeartbeatTransition {
    pub previous_status: i32,
    pub previous_health_state: String,
    pub status: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpiredClusterInstance {
    pub instance_uuid: String,
    pub name: String,
    pub cluster_uuid: String,
    pub host_uuid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpiredClusterHost {
    pub host_uuid: String,
    pub name: String,
    pub cluster_uuid: String,
}

#[derive(Clone, Copy, Debug)]
pub struct AuditLogWrite<'a> {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub operator_id: i64,
    pub operator_type: &'a str,
    pub action: &'a str,
    pub target_type: &'a str,
    pub target_id: Option<i64>,
    pub target_uuid: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub metadata_json: &'a str,
}

#[async_trait]
pub trait WebRepositoryPort: Send + Sync {
    async fn ready_check(&self) -> WebServiceResult<()>;

    async fn list_applications(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage>;

    async fn create_application(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        owner_id: Option<i64>,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn retrieve_application(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn update_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse>;

    async fn delete_application(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
    ) -> WebServiceResult<()>;

    async fn set_application_status(
        &self,
        tenant_id: i64,
        application_id: &str,
        status: i32,
    ) -> WebServiceResult<ApplicationResponse>;

    /// Atomic activation: the successful-deployment precondition and the
    /// status flip run in one transaction, so a concurrent deployment
    /// deletion can never slip between the check and the act.
    async fn activate_application(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse>;

    /// Resolves the backing site id for an application resource. The site is
    /// the internal carrier row; all child resources (domains, deployments,
    /// source versions, env variables, health checks) are site-scoped.
    async fn resolve_site_id(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<String>;

    async fn create_platform_target(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<PlatformTargetResponse>;

    async fn list_platform_targets(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<PlatformTargetPage>;

    async fn retrieve_platform_target(
        &self,
        tenant_id: i64,
        application_id: &str,
        platform_target_id: &str,
    ) -> WebServiceResult<PlatformTargetResponse>;

    async fn list_domains(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    /// Tenant-scoped domain inventory with optional owner filter, used by the
    /// application API to offer certificate issuance choices independently of
    /// application routing.
    async fn list_certificate_domains(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn retrieve_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_domain(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn prepare_domain_verification(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    async fn record_domain_verification_observation(
        &self,
        tenant_id: i64,
        challenge_id: &str,
        observation: &DomainVerificationObservation,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    /// Due domain-ownership challenges across every tenant, oldest first.
    ///
    /// The automatic verification sweep's work queue. Without it a challenge
    /// only advanced when an operator re-submitted the verify request, so a
    /// domain whose TXT record was published after the first check stayed
    /// `PENDING` forever — and `certificates.issue` requires `VERIFIED`
    /// hostnames, so no certificate could ever be issued for it.
    ///
    /// Rows are selected with `FOR UPDATE SKIP LOCKED` so a fleet of workers
    /// takes disjoint batches. A duplicate check in the rare interleaving is
    /// harmless: the observation write is keyed by challenge and re-reads the
    /// row under a lock, and a `VERIFIED` row is never downgraded.
    async fn due_domain_verification_challenges(
        &self,
        limit: i32,
    ) -> WebServiceResult<Vec<DomainVerificationChallenge>>;

    async fn list_root_domains(
        &self,
        tenant_id: i64,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<RootDomainPage>;

    async fn create_root_domain(
        &self,
        tenant_id: i64,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn retrieve_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn delete_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<()>;

    async fn update_root_domain(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &UpdateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse>;

    async fn list_root_domain_hostnames(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_root_domain_hostname(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn list_managed_domains(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage>;

    async fn create_managed_domain(
        &self,
        tenant_id: i64,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn delete_managed_domain(&self, tenant_id: i64, domain_id: &str) -> WebServiceResult<()>;

    async fn bind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<DomainResponse>;

    async fn unbind_managed_domain(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse>;

    async fn prepare_managed_domain_verification(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge>;

    async fn list_source_versions(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage>;

    async fn create_source_version(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
        retention_limit: i32,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn retrieve_source_version(
        &self,
        tenant_id: i64,
        application_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse>;

    async fn list_deployments(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage>;

    async fn create_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn retrieve_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn rollback_deployment(
        &self,
        tenant_id: i64,
        application_id: &str,
        deployment_id: &str,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<DeploymentResponse>;

    async fn list_env_variables(
        &self,
        tenant_id: i64,
        application_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<EnvVariablePage>;

    async fn create_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn update_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        variable_id: &str,
        request: &UpdateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse>;

    async fn delete_env_variable(
        &self,
        tenant_id: i64,
        application_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificates(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        site_id: Option<&str>,
        domain_id: Option<&str>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificatePage>;

    async fn enqueue_certificate_issue(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        requested_by: Option<i64>,
        request: &IssueCertificateRequest,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn enqueue_certificate_renewal(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        requested_by: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse>;

    async fn retrieve_certificate_operation(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        operation_id: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn schedule_due_certificate_renewals(
        &self,
        renew_before_days: u32,
        limit: i32,
    ) -> WebServiceResult<usize>;

    async fn claim_certificate_operations(
        &self,
        lease_owner: &str,
        lease_seconds: i64,
        limit: i32,
    ) -> WebServiceResult<Vec<CertificateOperationLease>>;

    async fn renew_certificate_operation_lease(
        &self,
        lease: &CertificateOperationLease,
        lease_seconds: i64,
    ) -> WebServiceResult<()>;

    async fn delete_certificate(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        deleted_by: Option<i64>,
    ) -> WebServiceResult<()>;

    async fn list_listener_certificate_bindings(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage>;

    async fn bind_listener_certificate(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse>;

    async fn unbind_listener_certificate(
        &self,
        tenant_id: i64,
        application_id: &str,
        domain_id: &str,
        binding_id: &str,
    ) -> WebServiceResult<()>;

    async fn finalize_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        update: &CertificateIssueUpdate,
    ) -> WebServiceResult<CertificateResponse>;

    async fn fail_certificate_operation(
        &self,
        lease: &CertificateOperationLease,
        failure_code: &str,
        failure_detail: Option<&str>,
        retry_at: &str,
        terminal_retry_at: &str,
    ) -> WebServiceResult<CertificateOperationResponse>;

    async fn update_certificate_auto_renew(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        auto_renew: bool,
    ) -> WebServiceResult<CertificateResponse>;

    /// Projects the decrypted TLS certificate material the node must serve
    /// into its self-hosted TLS runtime snapshot: every listener certificate
    /// binding on the node with a desired version, across all tenants the
    /// node serves.
    async fn load_node_tls_certificate_assignments(
        &self,
        node_uuid: &str,
    ) -> WebServiceResult<Vec<TlsCertificateAssignmentMaterial>>;

    /// Loads the revocation material (cert type and leaf chain) for a
    /// certificate, locking the aggregate so revocation is exclusive.
    async fn load_certificate_revocation_material(
        &self,
        tenant_id: i64,
        certificate_id: &str,
    ) -> WebServiceResult<CertificateRevocationMaterial>;

    /// Marks a certificate revoked after the CA acknowledged the revocation:
    /// stops auto-renewal, archives listener bindings, and records the
    /// revocation metadata.
    async fn mark_certificate_revoked(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        request: &RevokeCertificateRequest,
        revoked_by: Option<i64>,
    ) -> WebServiceResult<CertificateResponse>;

    /// Records the CA-suggested ARI renewal window on the certificate
    /// aggregate so the due-renewal scheduler prefers it over the fixed
    /// `renew_before_days` fallback.
    async fn record_certificate_renewal_info(
        &self,
        tenant_id: i64,
        certificate_id: &str,
        window_start: &str,
        window_end: &str,
    ) -> WebServiceResult<()>;

    async fn list_certificate_distribution(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificateDistributionPage>;

    async fn list_health_checks(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<HealthCheckPage>;

    async fn create_health_check(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<HealthCheckResponse>;

    async fn list_nginx_configs(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<NginxConfigPage>;

    async fn create_nginx_config(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn update_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn load_nginx_config_content(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<String>;

    /// Loads the content of the currently active Nginx configuration for a
    /// site, if one exists. Used to roll the edge back when a new activation
    /// succeeds at the edge but fails in the control plane.
    async fn load_active_nginx_config_content(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<Option<String>>;

    async fn resolve_site_primary_hostname(
        &self,
        tenant_id: i64,
        site_uuid: &str,
    ) -> WebServiceResult<String>;

    async fn webserver_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse>;

    async fn retrieve_nginx_status(
        &self,
        tenant_id: Option<i64>,
    ) -> WebServiceResult<NginxStatusResponse>;

    async fn list_servers(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ServerPage>;

    async fn create_server(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> WebServiceResult<CreateServerResponse>;

    async fn authenticate_agent_token(&self, token: &str) -> WebServiceResult<(String, i64)>;

    async fn resolve_runtime_assignment_target(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        node_uuid: &str,
    ) -> WebServiceResult<RuntimeAssignmentTarget>;

    async fn publish_runtime_assignment(
        &self,
        write: RuntimeAssignmentWrite,
    ) -> WebServiceResult<RuntimeAssignment>;

    async fn retrieve_current_runtime_assignment(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery>;

    async fn create_runtime_observation(
        &self,
        write: RuntimeObservationWrite,
    ) -> WebServiceResult<RuntimeObservation>;

    async fn retrieve_latest_runtime_observation(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation>;

    async fn record_agent_heartbeat(
        &self,
        server_id: &str,
        tenant_id: i64,
        request: &AgentHeartbeatRequest,
    ) -> WebServiceResult<AgentHeartbeatResponse>;

    async fn build_agent_sync_manifest(
        &self,
        server_id: &str,
        tenant_id: i64,
        if_sync_version: Option<&str>,
    ) -> WebServiceResult<AgentSyncResponse>;

    async fn list_audit_logs(
        &self,
        tenant_id: Option<i64>,
        query: &ListAuditLogsQuery,
    ) -> WebServiceResult<AuditLogPage>;

    async fn insert_audit_log(&self, entry: AuditLogWrite<'_>) -> WebServiceResult<()>;

    async fn list_clusters(&self, page: i32, page_size: i32) -> WebServiceResult<ClusterPage>;

    async fn create_cluster(
        &self,
        request: &CreateClusterRequest,
    ) -> WebServiceResult<ClusterResponse>;

    async fn retrieve_cluster(&self, cluster_id: &str) -> WebServiceResult<ClusterResponse>;

    async fn update_cluster(
        &self,
        cluster_id: &str,
        request: &UpdateClusterRequest,
    ) -> WebServiceResult<ClusterResponse>;

    async fn delete_cluster(&self, cluster_id: &str) -> WebServiceResult<()>;

    async fn list_cluster_hosts(
        &self,
        cluster_id: Option<&str>,
        status: Option<i32>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHostPage>;

    async fn retrieve_cluster_host(&self, host_id: &str) -> WebServiceResult<ClusterHostResponse>;

    async fn update_cluster_host(
        &self,
        host_id: &str,
        request: &UpdateClusterHostRequest,
    ) -> WebServiceResult<ClusterHostResponse>;

    async fn delete_cluster_host(&self, host_id: &str) -> WebServiceResult<()>;

    #[allow(clippy::too_many_arguments)]
    async fn list_cluster_instances(
        &self,
        cluster_id: Option<&str>,
        host_id: Option<&str>,
        status: Option<i32>,
        health_state: Option<&str>,
        join_mode: Option<i32>,
        sync_status: Option<i32>,
        // Label selector pairs: every pair must be present (`labels @>`).
        labels: &[(String, String)],
        // Free-text substring match over name / endpoint / hostname.
        search: Option<&str>,
        // Exact build version match (version-skew management).
        build_version: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterInstancePage>;

    async fn retrieve_cluster_instance(
        &self,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    async fn update_cluster_instance(
        &self,
        instance_id: &str,
        request: &UpdateClusterInstanceRequest,
    ) -> WebServiceResult<ClusterInstanceResponse>;

    async fn delete_cluster_instance(&self, instance_id: &str) -> WebServiceResult<()>;

    async fn list_cluster_events(
        &self,
        cluster_id: Option<&str>,
        severity: Option<&str>,
        instance_id: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterEventPage>;

    async fn cluster_overview(&self) -> WebServiceResult<ClusterOverviewResponse>;

    /// Resolves a cluster identity (internal + external ids) by its uuid;
    /// used by admin-plane sync publication addressed by the public id.
    async fn resolve_cluster_identity_by_uuid(
        &self,
        tenant_id: i64,
        cluster_uuid: &str,
    ) -> WebServiceResult<ClusterIdentity>;

    async fn resolve_cluster_identity(
        &self,
        tenant_id: i64,
        cluster_code: Option<&str>,
    ) -> WebServiceResult<ClusterIdentity>;

    async fn upsert_cluster_host(
        &self,
        write: ClusterHostUpsert,
    ) -> WebServiceResult<ClusterUpsert>;

    async fn upsert_cluster_instance(
        &self,
        write: ClusterInstanceUpsert,
    ) -> WebServiceResult<ClusterUpsert>;

    async fn authenticate_cluster_instance_token(
        &self,
        token: &str,
    ) -> WebServiceResult<ClusterInstanceCredentials>;

    async fn resolve_cluster_instance_by_uuid(
        &self,
        instance_uuid: &str,
    ) -> WebServiceResult<ClusterInstanceCredentials>;

    /// Next monotonic revision label (snowflake id string) for sync
    /// publication.
    async fn next_cluster_sync_revision_id(&self) -> WebServiceResult<String>;

    /// Node acknowledges graceful-drain completion: drains flag cleared,
    /// instance marked stopped (offline). Registry-driven, idempotent.
    async fn record_cluster_drain_complete(
        &self,
        tenant_id: i64,
        instance_uuid: &str,
    ) -> WebServiceResult<()>;

    /// Records one active-probe outcome and drives auto-eject / auto-recover.
    async fn record_cluster_probe_outcome(
        &self,
        write: ClusterProbeWrite,
    ) -> WebServiceResult<ClusterProbeOutcome>;

    /// Auto-discovery: reads the routeable instance inventory of one
    /// cluster (online, routing-enabled, not draining, not ejected) together
    /// with the cluster's LB strategy and served domains. `None` when the
    /// cluster code is unknown.
    async fn discover_cluster_routing(
        &self,
        cluster_code: &str,
    ) -> WebServiceResult<Option<ClusterRoutingDiscovery>>;

    /// Latest desired revision per sync kind for a cluster (empty when the
    /// cluster never published one).
    async fn latest_cluster_sync_desired(
        &self,
        tenant_id: i64,
        cluster_id: i64,
    ) -> WebServiceResult<Vec<ClusterSyncDesired>>;

    /// Payload of one stored sync revision, addressed by revision label.
    async fn cluster_sync_revision_payload(
        &self,
        tenant_id: i64,
        cluster_id: i64,
        kind: i32,
        revision: &str,
    ) -> WebServiceResult<Option<ClusterSyncRevisionPayload>>;

    /// Publishes one desired-state revision (admin action). The store marks
    /// the revision as desired for every non-deleted instance of the
    /// cluster by bumping its per-kind desired column and flipping the
    /// aggregate sync status to PENDING.
    async fn publish_cluster_sync_revision(
        &self,
        write: ClusterSyncRevisionPublish,
    ) -> WebServiceResult<u64>;

    /// Records one instance sync acknowledgment, updating the per-kind
    /// applied revision and recomputing the aggregate sync status.
    async fn record_cluster_sync_ack(&self, write: ClusterSyncAckWrite) -> WebServiceResult<()>;

    async fn record_cluster_heartbeat(
        &self,
        write: ClusterHeartbeatWrite,
    ) -> WebServiceResult<ClusterHeartbeatTransition>;

    async fn list_cluster_peers(
        &self,
        exclude_instance_uuid: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ClusterPeer>>;

    async fn claim_cluster_peer_messages(
        &self,
        instance_id: i64,
        limit: i32,
        now: &str,
    ) -> WebServiceResult<Vec<ClusterPeerMessage>>;

    async fn enqueue_cluster_peer_messages(
        &self,
        write: ClusterPeerMessageEnqueue<'_>,
    ) -> WebServiceResult<u64>;

    async fn list_cluster_heartbeats(
        &self,
        instance_uuid: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage>;

    async fn record_cluster_event(&self, write: ClusterEventWrite<'_>) -> WebServiceResult<()>;

    async fn expire_stale_cluster_instances(
        &self,
        now: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ExpiredClusterInstance>>;

    async fn expire_stale_cluster_hosts(
        &self,
        now: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ExpiredClusterHost>>;

    async fn expire_cluster_peer_messages(&self, now: &str, limit: i32) -> WebServiceResult<u64>;

    async fn purge_cluster_heartbeats(&self, older_than: &str, limit: i32)
        -> WebServiceResult<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustive over the whole status domain rather than sampled pairs: the
    /// rule is "maintenance survives every machine writer", and a sampled test
    /// cannot show that no other value became sticky by accident.
    #[test]
    fn only_operator_maintenance_survives_a_machine_status_write() {
        for previous in 0..=5 {
            for proposed in 0..=5 {
                let recorded = cluster_operator_owned_status(previous, proposed);
                if proposed == CLUSTER_INSTANCE_STATUS_MAINTENANCE {
                    assert_eq!(
                        recorded, CLUSTER_INSTANCE_STATUS_MAINTENANCE,
                        "an operator writing maintenance back must be stored as-is"
                    );
                } else if previous == CLUSTER_INSTANCE_STATUS_MAINTENANCE {
                    assert_eq!(
                        recorded, CLUSTER_INSTANCE_STATUS_MAINTENANCE,
                        "nothing but an operator may clear maintenance (previous={previous}, proposed={proposed})"
                    );
                } else {
                    assert_eq!(
                        recorded, proposed,
                        "every other status stays last-writer-wins (previous={previous})"
                    );
                }
            }
        }
    }

    /// The three machine writers, by the value each proposes.
    #[test]
    fn machine_writers_cannot_end_an_operator_maintenance_mark() {
        let maintenance = CLUSTER_INSTANCE_STATUS_MAINTENANCE;
        // The node's heartbeat reporter claims `online`…
        assert_eq!(cluster_operator_owned_status(maintenance, 1), maintenance);
        // …its registration after a restart claims `online` too…
        assert_eq!(cluster_operator_owned_status(maintenance, 1), maintenance);
        // …and the prober ejects with `error`.
        assert_eq!(cluster_operator_owned_status(maintenance, 4), maintenance);
        // A fresh row is not in maintenance, so the machine writers still own
        // the ordinary status: nothing here made `online` sticky.
        assert_eq!(cluster_operator_owned_status(0, 1), 1);
        assert_eq!(cluster_operator_owned_status(1, 0), 0);
        assert_eq!(cluster_operator_owned_status(1, 4), 4);
    }
}
