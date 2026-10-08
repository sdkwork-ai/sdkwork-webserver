//! Web business service orchestrating repository ports and HTTP API traits.

pub mod agent_ops;
pub mod app;
pub mod audit_time;
pub mod backend;
pub mod certificate_ops;
pub mod certificate_renewal_ops;
pub mod cluster_ops;
pub mod domain_verification;
pub mod nginx_ops;
pub mod repository;
pub mod runtime_assignment_ops;
pub mod source_import;
pub mod tls_material_distribution;

pub use cluster_ops::{ClusterMachineCredential, ClusterSweepReport};
pub use domain_verification::{
    DnsTxtDomainOwnershipVerifier, DomainOwnershipVerifier, DomainVerificationCycleReport,
};
pub use repository::{
    AuditLogWrite, CLUSTER_INSTANCE_STATUS_ERROR, CLUSTER_INSTANCE_STATUS_MAINTENANCE,
    CLUSTER_INSTANCE_STATUS_OFFLINE, CLUSTER_INSTANCE_STATUS_ONLINE, CertificateRevocationMaterial,
    ClusterEventWrite, ClusterHeartbeatTransition, ClusterHeartbeatWrite, ClusterHostUpsert,
    ClusterIdentity, ClusterInstanceCredentials, ClusterInstanceUpsert, ClusterPeerMessageEnqueue,
    ClusterProbeOutcome, ClusterProbeWrite, ClusterRoutingDiscovery, ClusterRoutingInstance,
    ClusterSyncAckWrite, ClusterSyncDesired, ClusterSyncRevisionPayload,
    ClusterSyncRevisionPublish, ClusterUpsert, DnsRecordSnapshotRow, DomainDnsRecordFilter,
    DomainDnsRecordUpsert, DomainDnsSnapshotWrite, DomainHostnameAsset,
    DomainVerificationChallenge, DomainVerificationObservation, ExpiredClusterHost,
    ExpiredClusterInstance, RootDomainDnsSyncTarget, RuntimeAssignmentTarget,
    RuntimeAssignmentWrite, RuntimeObservationWrite, WebRepositoryPort, cluster_operator_owned_status,
    domain_asset_for_record, hostname_matches_record,
};
pub use source_import::{
    ApplicationSourceImporter, GitSourceImportRequest, ImportedApplicationSource,
};
pub use tls_material_distribution::TlsMaterialDistributionConfig;

use std::sync::Arc;

use sdkwork_webserver_acme_service::CertificateIssuer;
use sdkwork_webserver_contract::{
    MetricsSummaryReadPort, TrafficUsageReadPort, WebServiceError, WebServiceResult,
};
use sdkwork_webserver_edge_runtime::EdgeRuntime;

/// Application service for SDKWork Web control plane operations.
pub struct WebService {
    pub(crate) repository: Arc<dyn WebRepositoryPort>,
    pub(crate) certificate_issuer: Arc<CertificateIssuer>,
    pub(crate) edge_runtime: Arc<EdgeRuntime>,
    pub(crate) source_importer: Arc<dyn ApplicationSourceImporter>,
    pub(crate) domain_ownership_verifier: Arc<dyn DomainOwnershipVerifier>,
    /// Read model for aggregated traffic usage. The facts live in the Deploy
    /// control plane, so the host injects an adapter after construction rather
    /// than this crate depending on another module's persistence layer. Absent
    /// means the statistics endpoint reports an unavailable capability instead
    /// of an empty chart, which would read as "no traffic" rather than
    /// "not wired".
    pub(crate) traffic_usage: Option<Arc<dyn TrafficUsageReadPort>>,
    /// Read model for the dashboard metric summary. Same shape and same reason
    /// as `traffic_usage`: part of the reading belongs to another module (the
    /// IAM subjects and the metered facts), so the host injects the adapter
    /// after construction. Absent means the operation reports an unavailable
    /// capability rather than a row of zeros, which would read as "this
    /// installation has no users" rather than "not wired".
    pub(crate) metrics_summary: Option<Arc<dyn MetricsSummaryReadPort>>,
}

impl WebService {
    pub fn new(
        repository: Arc<dyn WebRepositoryPort>,
        certificate_issuer: Arc<CertificateIssuer>,
        edge_runtime: Arc<EdgeRuntime>,
    ) -> Result<Self, WebServiceError> {
        Self::new_with_source_importer(
            repository,
            certificate_issuer,
            edge_runtime,
            Arc::new(source_import::UnavailableApplicationSourceImporter),
        )
    }

    pub fn new_with_source_importer(
        repository: Arc<dyn WebRepositoryPort>,
        certificate_issuer: Arc<CertificateIssuer>,
        edge_runtime: Arc<EdgeRuntime>,
        source_importer: Arc<dyn ApplicationSourceImporter>,
    ) -> Result<Self, WebServiceError> {
        Ok(Self::new_with_dependencies(
            repository,
            certificate_issuer,
            edge_runtime,
            source_importer,
            Arc::new(DnsTxtDomainOwnershipVerifier::new()?),
        ))
    }

    pub fn new_with_dependencies(
        repository: Arc<dyn WebRepositoryPort>,
        certificate_issuer: Arc<CertificateIssuer>,
        edge_runtime: Arc<EdgeRuntime>,
        source_importer: Arc<dyn ApplicationSourceImporter>,
        domain_ownership_verifier: Arc<dyn DomainOwnershipVerifier>,
    ) -> Self {
        Self {
            repository,
            certificate_issuer,
            edge_runtime,
            source_importer,
            domain_ownership_verifier,
            traffic_usage: None,
            metrics_summary: None,
        }
    }

    /// Injects the aggregated traffic usage read model.
    ///
    /// Consuming builder rather than another `new_with_*` parameter: the port
    /// depends on a module this service deliberately does not depend on, so
    /// every existing construction site keeps working and only a host that can
    /// actually supply the adapter has to know about it.
    pub fn with_traffic_usage_reader(mut self, port: Arc<dyn TrafficUsageReadPort>) -> Self {
        self.traffic_usage = Some(port);
        self
    }

    /// Injects the dashboard metric summary read model.
    ///
    /// A second consumed builder rather than a shared one: the two readings are
    /// assembled from different sources and a host may legitimately have one
    /// without the other (a deployment with usage facts but no IAM tables in
    /// this database, or the reverse), and each must report its own absence.
    pub fn with_metrics_summary_reader(mut self, port: Arc<dyn MetricsSummaryReadPort>) -> Self {
        self.metrics_summary = Some(port);
        self
    }

    pub async fn ready_check(&self) -> WebServiceResult<()> {
        self.repository.ready_check().await
    }

    /// Persists an audit log entry. A persistence failure is counted on the
    /// process-global counter (exported on the operations `/metrics` scrape
    /// as `sdkwork_webserver_audit_persistence_failures_total` and alerted on
    /// by the shipped Prometheus rules) so the audit gap is observable;
    /// business operations do not fail after their durable effect has already
    /// been committed.
    pub async fn record_audit_log(&self, entry: AuditLogWrite<'_>) -> WebServiceResult<()> {
        match self.repository.insert_audit_log(entry).await {
            Ok(()) => Ok(()),
            Err(error) => {
                sdkwork_webserver_contract::observability::record_audit_persistence_failure();
                tracing::error!(
                    error = ?error,
                    "failed to persist audit log entry; audit persistence failures now {}",
                    self.audit_persistence_failures()
                );
                Err(error)
            }
        }
    }

    /// Total audit log persistence failures since process start. Operators
    /// must alert on a nonzero value; a silent audit gap violates the
    /// commercial audit contract.
    pub fn audit_persistence_failures(&self) -> u64 {
        sdkwork_webserver_contract::observability::audit_persistence_failures_total()
    }
}
