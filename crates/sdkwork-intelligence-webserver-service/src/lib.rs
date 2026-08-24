//! Web business service orchestrating repository ports and HTTP API traits.

pub mod agent_ops;
pub mod app;
pub mod audit_time;
pub mod backend;
pub mod certificate_ops;
pub mod certificate_renewal_ops;
pub mod domain_verification;
pub mod nginx_ops;
pub mod repository;
pub mod runtime_assignment_ops;
pub mod source_import;
pub mod tls_material_distribution;

pub use domain_verification::{DnsTxtDomainOwnershipVerifier, DomainOwnershipVerifier};
pub use repository::{
    AuditLogWrite, CertificateRevocationMaterial, DomainVerificationChallenge,
    DomainVerificationObservation, RuntimeAssignmentTarget, RuntimeAssignmentWrite,
    RuntimeObservationWrite, WebRepositoryPort,
};
pub use source_import::{
    ApplicationSourceImporter, GitSourceImportRequest, ImportedApplicationSource,
};
pub use tls_material_distribution::TlsMaterialDistributionConfig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sdkwork_webserver_acme_service::CertificateIssuer;
use sdkwork_webserver_contract::WebServiceResult;
use sdkwork_webserver_edge_runtime::EdgeRuntime;

/// Application service for SDKWork Web control plane operations.
pub struct WebService {
    pub(crate) repository: Arc<dyn WebRepositoryPort>,
    pub(crate) certificate_issuer: Arc<CertificateIssuer>,
    pub(crate) edge_runtime: Arc<EdgeRuntime>,
    pub(crate) source_importer: Arc<dyn ApplicationSourceImporter>,
    pub(crate) domain_ownership_verifier: Arc<dyn DomainOwnershipVerifier>,
    /// Count of audit log persistence failures so the audit gap stays
    /// observable through health/readiness surfaces instead of being silent.
    audit_persistence_failures: AtomicU64,
}

impl WebService {
    pub fn new(
        repository: Arc<dyn WebRepositoryPort>,
        certificate_issuer: Arc<CertificateIssuer>,
        edge_runtime: Arc<EdgeRuntime>,
    ) -> Self {
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
    ) -> Self {
        Self::new_with_dependencies(
            repository,
            certificate_issuer,
            edge_runtime,
            source_importer,
            Arc::new(DnsTxtDomainOwnershipVerifier::new()),
        )
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
            audit_persistence_failures: AtomicU64::new(0),
        }
    }

    pub async fn ready_check(&self) -> WebServiceResult<()> {
        self.repository.ready_check().await
    }

    /// Persists an audit log entry. A persistence failure is counted and
    /// surfaced through [`Self::audit_persistence_failures`] so the audit
    /// gap is observable; business operations do not fail after their
    /// durable effect has already been committed.
    pub async fn record_audit_log(&self, entry: AuditLogWrite<'_>) -> WebServiceResult<()> {
        match self.repository.insert_audit_log(entry).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.audit_persistence_failures
                    .fetch_add(1, Ordering::Relaxed);
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
        self.audit_persistence_failures.load(Ordering::Relaxed)
    }
}
