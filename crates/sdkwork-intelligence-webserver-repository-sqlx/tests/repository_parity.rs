use std::collections::HashSet;
use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_sqlx::create_pool_from_config;
use sdkwork_intelligence_webserver_repository_sqlx::PostgresWebRepository;
use sdkwork_intelligence_webserver_service::{
    AuditLogWrite, DomainVerificationChallenge, DomainVerificationObservation,
    RuntimeAssignmentTarget, RuntimeAssignmentWrite, RuntimeObservationWrite, WebRepositoryPort,
};
use sdkwork_webserver_contract::{
    AgentCertificateObservation, AgentHeartbeatRequest, ApplicationStoreListing,
    CertificateIssueUpdate, CertificateOperationLease, CreateApplicationRequest,
    CreateDeploymentRequest, CreateDomainRequest, CreateEnvVariableRequest,
    CreateHealthCheckRequest, CreateListenerCertificateBindingRequest, CreateManagedDomainRequest,
    CreateNginxConfigRequest, CreateRootDomainHostnameRequest, CreateRootDomainRequest,
    CreateServerRequest, CreateSourceVersionRequest, IssueCertificateRequest,
    ListApplicationsQuery, ListAuditLogsQuery, ListNginxConfigsQuery, ListRootDomainsQuery,
    MediaResource, RevokeCertificateRequest, RuntimeObservationState, SourceVersionConfigSnapshot,
    UpdateApplicationRequest, UpdateDomainApplicationBindingRequest, UpdateNginxConfigRequest,
    WebServiceErrorKind, WebsiteRuntimeSetSnapshot,
};
use sdkwork_webserver_core::website_runtime::website_runtime_set_snapshot_sha256;
use sdkwork_webserver_database_host::bootstrap_web_database;
use sqlx::{PgPool, Row};

const POSTGRES_TEST_URL_ENV: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";
const TENANT_A: i64 = 410_001;
const TENANT_B: i64 = 410_002;

async fn verify_site_domain_with_evidence(
    repository: &Arc<dyn WebRepositoryPort>,
    tenant_id: i64,
    site_id: &str,
    domain_id: &str,
) -> DomainVerificationChallenge {
    let challenge = repository
        .prepare_domain_verification(tenant_id, site_id, domain_id)
        .await
        .expect("prepare domain verification challenge");
    repository
        .record_domain_verification_observation(
            tenant_id,
            &challenge.challenge_id,
            &DomainVerificationObservation {
                observed_sha256: Some(challenge.proof_sha256.clone()),
                failure_code: None,
            },
        )
        .await
        .expect("record matching domain verification evidence")
}

async fn verify_managed_domain_with_evidence(
    repository: &Arc<dyn WebRepositoryPort>,
    tenant_id: i64,
    domain_id: &str,
) -> DomainVerificationChallenge {
    let challenge = repository
        .prepare_managed_domain_verification(tenant_id, domain_id)
        .await
        .expect("prepare managed domain verification challenge");
    repository
        .record_domain_verification_observation(
            tenant_id,
            &challenge.challenge_id,
            &DomainVerificationObservation {
                observed_sha256: Some(challenge.proof_sha256.clone()),
                failure_code: None,
            },
        )
        .await
        .expect("record matching managed domain verification evidence")
}

async fn enqueue_and_claim_certificate(
    repository: &Arc<dyn WebRepositoryPort>,
    tenant_id: i64,
    owner_id: Option<i64>,
    requested_by: Option<i64>,
    request: &IssueCertificateRequest,
    idempotency_key: &str,
    lease_owner: &str,
) -> CertificateOperationLease {
    let operation = repository
        .enqueue_certificate_issue(
            tenant_id,
            owner_id,
            requested_by,
            request,
            Some(idempotency_key),
        )
        .await
        .expect("enqueue certificate operation");
    repository
        .claim_certificate_operations(lease_owner, 60, 32)
        .await
        .expect("claim certificate operation")
        .into_iter()
        .find(|lease| lease.operation_id == operation.operation_id)
        .expect("enqueued certificate operation must be claimable")
}

fn test_certificate_update(
    cert_name: &str,
    cert_type: i32,
    key_algorithm: &str,
    hash_marker: char,
    auto_renew: bool,
) -> CertificateIssueUpdate {
    CertificateIssueUpdate {
        cert_name: cert_name.to_string(),
        cert_type,
        issuer: "SDKWork Test CA".to_string(),
        subject: format!("CN={cert_name}"),
        serial_sha256: hash_marker.to_string().repeat(64),
        fingerprint_sha256: hash_marker.to_string().repeat(64),
        spki_sha256: hash_marker.to_string().repeat(64),
        chain_sha256: hash_marker.to_string().repeat(64),
        key_algorithm: key_algorithm.to_string(),
        fullchain_pem: format!(
            "-----BEGIN CERTIFICATE-----\n{cert_name}\n-----END CERTIFICATE-----\n"
        ),
        private_key_pem: format!(
            "-----BEGIN PRIVATE KEY-----\n{cert_name}\n-----END PRIVATE KEY-----\n"
        ),
        not_before: "2026-01-01T00:00:00Z".to_string(),
        not_after: "2027-01-01T00:00:00Z".to_string(),
        auto_renew,
    }
}

struct EnvironmentVariableGuard {
    key: &'static str,
    previous_value: Option<std::ffi::OsString>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var_os(key);
        std::env::set_var(key, value);
        Self {
            key,
            previous_value,
        }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        match &self.previous_value {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

struct TestContext {
    pool: PgPool,
    repository: Arc<dyn WebRepositoryPort>,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL database"]
async fn postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sdkwork_intelligence_webserver_repository_sqlx=error")
        .with_test_writer()
        .try_init();
    let url = std::env::var(POSTGRES_TEST_URL_ENV).unwrap_or_else(|_| {
        panic!("set {POSTGRES_TEST_URL_ENV} to a disposable empty PostgreSQL database")
    });
    assert!(
        url.starts_with("postgres://") || url.starts_with("postgresql://"),
        "{POSTGRES_TEST_URL_ENV} must be a PostgreSQL URL"
    );
    let context = prepare_database(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url,
        max_connections: 4,
        ..Default::default()
    })
    .await;

    verify_repository_contract(&context).await;
    verify_certificate_activation_compensation(&context).await;
    verify_certificate_revocation_ari_and_tls_projection(&context).await;
    context.pool.close().await;
}

async fn verify_certificate_revocation_ari_and_tls_projection(context: &TestContext) {
    let repository = &context.repository;
    let site = repository
        .create_application(
            TENANT_A,
            Some(31),
            Some(91),
            &CreateApplicationRequest {
                name: "Parity Revocation Site".to_string(),
                slug: Some("parity-revocation".to_string()),
                description: None,
                app_kind: "WEB".to_string(),
                runtime_config: None,
                store_listing: None,
            },
        )
        .await
        .expect("create revocation site");
    let domain = repository
        .create_domain(
            TENANT_A,
            &site.id,
            &CreateDomainRequest {
                hostname: "revoke.example.test".to_string(),
                is_primary: true,
                ssl_enabled: true,
                ssl_provider: Some("self-signed".to_string()),
            },
        )
        .await
        .expect("create revocation domain");
    verify_site_domain_with_evidence(&context.repository, TENANT_A, &site.id, &domain.id).await;

    // 1) Revocation lifecycle: issue a self-signed certificate, bind it to a
    // listener, revoke it, and prove the bindings are archived and the
    // aggregate is terminal.
    let revoke_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        Some(91),
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![domain.id.clone()],
            cert_type: 3,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: false,
        },
        "parity-certificate-revoke",
        "repository-parity-revoke",
    )
    .await;
    let revoked_certificate = repository
        .finalize_certificate_operation(
            &revoke_lease,
            &test_certificate_update("revoke.example.test", 3, "ECDSA", '7', false),
        )
        .await
        .expect("finalize revoke certificate");
    assert_eq!(revoked_certificate.status, "ISSUED");
    let revoke_binding = repository
        .bind_listener_certificate(
            TENANT_A,
            &site.id,
            &domain.id,
            &CreateListenerCertificateBindingRequest {
                certificate_id: revoke_lease.certificate_id.clone(),
                certificate_version_id: None,
                priority: 100,
                is_default: true,
            },
        )
        .await
        .expect("bind certificate for revocation");
    assert_eq!(revoke_binding.status, "PENDING");

    let revocation_material = repository
        .load_certificate_revocation_material(TENANT_A, &revoke_lease.certificate_id)
        .await
        .expect("load revocation material");
    assert_eq!(revocation_material.cert_type, 3);
    assert!(revocation_material
        .fullchain_pem
        .contains("BEGIN CERTIFICATE"));

    let revoked = repository
        .mark_certificate_revoked(
            TENANT_A,
            &revoke_lease.certificate_id,
            &RevokeCertificateRequest {
                reason: "superseded".to_string(),
            },
            Some(91),
        )
        .await
        .expect("mark certificate revoked");
    assert_eq!(revoked.status, "REVOKED");
    assert!(
        repository
            .mark_certificate_revoked(
                TENANT_A,
                &revoke_lease.certificate_id,
                &RevokeCertificateRequest {
                    reason: "superseded".to_string()
                },
                Some(91),
            )
            .await
            .is_err(),
        "a second revocation of an inactive certificate must conflict"
    );
    let archived_bindings = repository
        .list_listener_certificate_bindings(TENANT_A, &site.id, &domain.id, 1, 20)
        .await
        .expect("list bindings after revocation");
    assert!(
        archived_bindings
            .items
            .iter()
            .all(|binding| binding.status == "ARCHIVED"),
        "revocation must archive every listener binding"
    );

    // 2) ARI scheduling: a future CA-suggested window suppresses scheduling
    // inside the fixed window; an elapsed window schedules immediately.
    let ari_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        Some(91),
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![domain.id.clone()],
            cert_type: 1,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: true,
        },
        "parity-certificate-ari",
        "repository-parity-ari",
    )
    .await;
    let ari_certificate = repository
        .finalize_certificate_operation(
            &ari_lease,
            &test_certificate_update("ari.example.test", 1, "ECDSA", '8', true),
        )
        .await
        .expect("finalize ARI certificate");
    assert_eq!(ari_certificate.status, "ISSUED");
    repository
        .record_certificate_renewal_info(
            TENANT_A,
            &ari_lease.certificate_id,
            "2099-01-01T00:00:00Z",
            "2099-02-01T00:00:00Z",
        )
        .await
        .expect("record future ARI window");
    let scheduled_with_future_ari = repository
        .schedule_due_certificate_renewals(365, 100)
        .await
        .expect("schedule with future ARI window");
    repository
        .record_certificate_renewal_info(
            TENANT_A,
            &ari_lease.certificate_id,
            "2020-01-01T00:00:00Z",
            "2020-02-01T00:00:00Z",
        )
        .await
        .expect("record elapsed ARI window");
    let scheduled_with_elapsed_ari = repository
        .schedule_due_certificate_renewals(365, 100)
        .await
        .expect("schedule with elapsed ARI window");
    assert!(
        scheduled_with_elapsed_ari > scheduled_with_future_ari,
        "an elapsed ARI window must schedule renewal even inside the fixed window"
    );

    // 3) Node TLS material projection: only active bindings on assigned sites
    // are projected; revoked certificates never appear.
    let ari_domain = repository
        .create_domain(
            TENANT_A,
            &site.id,
            &CreateDomainRequest {
                hostname: "ari.example.test".to_string(),
                is_primary: false,
                ssl_enabled: true,
                ssl_provider: Some("lets-encrypt".to_string()),
            },
        )
        .await
        .expect("create ARI domain");
    verify_site_domain_with_evidence(&context.repository, TENANT_A, &site.id, &ari_domain.id).await;
    let _ari_binding = repository
        .bind_listener_certificate(
            TENANT_A,
            &site.id,
            &ari_domain.id,
            &CreateListenerCertificateBindingRequest {
                certificate_id: ari_lease.certificate_id.clone(),
                certificate_version_id: None,
                priority: 100,
                is_default: true,
            },
        )
        .await
        .expect("bind ARI certificate to listener");

    let server = repository
        .create_server(
            TENANT_A,
            &CreateServerRequest {
                name: "Parity TLS Node".to_string(),
                host: "192.0.2.46".to_string(),
                tenant_scope_hash: "a".repeat(64),
                ssh_port: 22,
            },
        )
        .await
        .expect("create TLS projection node");
    let target = repository
        .resolve_runtime_assignment_target(TENANT_A, false, &server.server.id)
        .await
        .expect("resolve TLS node runtime target");
    repository
        .publish_runtime_assignment(runtime_assignment_write(
            &target,
            "production",
            1,
            "parity-tls-projection",
        ))
        .await
        .expect("publish TLS node runtime assignment");
    sqlx::query(
        "UPDATE web_runtime_assignment a
         SET runtime_set = jsonb_set(
             a.runtime_set,
             '{descriptors}',
             jsonb_build_array(jsonb_build_object('siteUuid', CAST($3 AS TEXT))),
             FALSE
         )
         FROM web_server s
         WHERE a.tenant_id = $1 AND a.server_id = s.id AND s.uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&server.server.id)
    .bind(&site.id)
    .execute(&context.pool)
    .await
    .expect("scope TLS node assignment to the revocation site");

    let assignments = repository
        .load_node_tls_certificate_assignments(&server.server.id)
        .await
        .expect("load node TLS certificate assignments");
    assert_eq!(
        assignments.len(),
        1,
        "only the active listener binding is projected"
    );
    assert_eq!(assignments[0].certificate_id, ari_lease.certificate_id);
    assert_eq!(assignments[0].cert_name, "ari.example.test");
    assert_eq!(
        assignments[0].hostnames,
        vec!["ari.example.test".to_string()]
    );
    assert!(assignments[0].fullchain_pem.contains("BEGIN CERTIFICATE"));
    assert!(assignments[0].private_key_pem.contains("PRIVATE KEY"));
    assert!(assignments[0].not_before.ends_with('Z'));
    assert!(assignments[0].not_after.ends_with('Z'));
    assert_eq!(assignments[0].fingerprint_sha256.len(), 64);
    assert!(
        !assignments
            .iter()
            .any(|assignment| assignment.certificate_id == revoke_lease.certificate_id),
        "revoked certificates must never be projected into node TLS material"
    );
    assert!(repository
        .load_node_tls_certificate_assignments("unknown-node")
        .await
        .expect("unknown node yields no assignments")
        .is_empty());
}

async fn verify_certificate_activation_compensation(context: &TestContext) {
    let site = context
        .repository
        .create_application(
            TENANT_A,
            Some(31),
            Some(93),
            &CreateApplicationRequest {
                name: "Certificate Compensation Site".to_string(),
                slug: Some("certificate-compensation".to_string()),
                description: None,
                app_kind: "WEB".to_string(),
                runtime_config: None,
                store_listing: None,
            },
        )
        .await
        .expect("create certificate compensation site");
    let domain = context
        .repository
        .create_domain(
            TENANT_A,
            &site.id,
            &CreateDomainRequest {
                hostname: "compensation.example.test".to_string(),
                is_primary: true,
                ssl_enabled: true,
                ssl_provider: Some("self-signed".to_string()),
            },
        )
        .await
        .expect("create certificate compensation domain");
    verify_site_domain_with_evidence(&context.repository, TENANT_A, &site.id, &domain.id).await;

    let operation = context
        .repository
        .enqueue_certificate_issue(
            TENANT_A,
            Some(93),
            Some(93),
            &IssueCertificateRequest {
                domain_ids: vec![domain.id],
                cert_type: 3,
                key_algorithm: "ECDSA".to_string(),
                auto_renew: false,
            },
            Some("certificate-compensation"),
        )
        .await
        .expect("enqueue compensation certificate operation");
    sqlx::query(
        "UPDATE web_certificate_operation SET max_attempts = 1 WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&operation.operation_id)
    .execute(&context.pool)
    .await
    .expect("bound compensation operation retry budget");
    let lease = context
        .repository
        .claim_certificate_operations("repository-compensation", 60, 1)
        .await
        .expect("claim compensation certificate operation")
        .into_iter()
        .next()
        .expect("compensation operation must be claimable");

    install_certificate_finalize_failure_trigger(&context.pool).await;
    let result = context
        .repository
        .finalize_certificate_operation(
            &lease,
            &test_certificate_update("compensation.example.test", 3, "ECDSA", 'a', false),
        )
        .await;
    assert_eq!(
        result
            .expect_err("database finalization failure must fail issuance")
            .kind(),
        WebServiceErrorKind::Internal
    );

    let version_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_certificate_version version
         INNER JOIN web_certificate certificate ON certificate.id = version.certificate_id
         WHERE certificate.tenant_id = $1 AND certificate.uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&lease.certificate_id)
    .fetch_one(&context.pool)
    .await
    .expect("count rolled-back certificate versions");
    assert_eq!(
        version_count, 0,
        "certificate version and operation completion must roll back together"
    );
    context
        .repository
        .fail_certificate_operation(
            &lease,
            "CERTIFICATE_FINALIZATION_FAILED",
            "2099-01-01T00:00:00Z",
            "2099-01-02T00:00:00Z",
        )
        .await
        .expect("persist bounded terminal compensation failure");
    let row = sqlx::query(
        "SELECT status, renewal_status, CAST(metadata AS TEXT) AS metadata
         FROM web_certificate WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&lease.certificate_id)
    .fetch_one(&context.pool)
    .await
    .expect("load compensated certificate row");
    assert_eq!(row.try_get::<i32, _>("status").expect("status"), 0);
    assert_eq!(
        row.try_get::<i32, _>("renewal_status")
            .expect("renewal status"),
        3
    );
    let metadata: String = row.try_get("metadata").expect("failure metadata");
    assert!(metadata.contains("CERTIFICATE_FINALIZATION_FAILED"));
    assert!(!metadata.contains("forced certificate finalize failure"));
    remove_certificate_finalize_failure_trigger(&context.pool).await;
}

async fn prepare_database(config: DatabaseConfig) -> TestContext {
    let lifecycle_pool = create_pool_from_config(config.clone())
        .await
        .expect("create lifecycle pool");
    let pool = lifecycle_pool
        .as_postgres()
        .expect("PostgreSQL lifecycle pool")
        .clone();
    let existing_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect disposable PostgreSQL schema");
    assert_eq!(
        existing_tables, 0,
        "refusing to run repository parity against a non-empty PostgreSQL schema"
    );
    let _auto_migrate = EnvironmentVariableGuard::set("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    bootstrap_web_database(lifecycle_pool)
        .await
        .expect("initialize PostgreSQL Web database lifecycle");

    let _database_engine = config.engine;
    let id_generator = SnowflakeIdGenerator::new(731).expect("create test Snowflake generator");
    let repository = Arc::new(PostgresWebRepository::new(
        pool.clone(),
        id_generator,
        [0x5a; 32],
    )) as Arc<dyn WebRepositoryPort>;
    TestContext { pool, repository }
}

async fn verify_repository_contract(context: &TestContext) {
    let repository = &context.repository;
    let mut sites = Vec::new();
    for index in 0..4 {
        sites.push(
            repository
                .create_application(
                    TENANT_A,
                    Some(31),
                    Some(91),
                    &CreateApplicationRequest {
                        name: format!("Alpha Site {index}"),
                        slug: Some(format!("alpha-{index}")),
                        description: None,
                        app_kind: "WEB".to_owned(),
                        runtime_config: None,
                        store_listing: None,
                    },
                )
                .await
                .expect("create tenant A site"),
        );
    }
    let api_application = repository
        .create_application(
            TENANT_A,
            Some(31),
            Some(91),
            &CreateApplicationRequest {
                name: "Alpha API".to_owned(),
                slug: Some("alpha-api".to_owned()),
                description: None,
                app_kind: "API".to_owned(),
                runtime_config: None,
                store_listing: None,
            },
        )
        .await
        .expect("create API application");
    assert_eq!(api_application.app_kind, "API");
    let api_page = repository
        .list_applications(
            TENANT_A,
            None,
            &ListApplicationsQuery {
                page: 1,
                page_size: 20,
                status: Some(0),
                application_type: Some("API".to_owned()),
                site_type: None,
                keyword: Some("alpha".to_owned()),
            },
        )
        .await
        .expect("filter API applications");
    assert_eq!(api_page.total, 1);
    assert_eq!(api_page.items[0].id, api_application.id);
    let owner_page = repository
        .list_applications(
            TENANT_A,
            Some(91),
            &ListApplicationsQuery {
                page: 1,
                page_size: 100,
                status: None,
                application_type: None,
                site_type: None,
                keyword: None,
            },
        )
        .await
        .expect("list applications for the owning user");
    assert_eq!(owner_page.total, 5);
    assert!(repository
        .list_applications(
            TENANT_A,
            Some(92),
            &ListApplicationsQuery {
                page: 1,
                page_size: 100,
                status: None,
                application_type: None,
                site_type: None,
                keyword: None,
            },
        )
        .await
        .expect("list applications for another user")
        .items
        .is_empty());
    repository
        .retrieve_application(TENANT_A, Some(92), &api_application.id)
        .await
        .expect_err("another user must not retrieve an owner-scoped application");
    repository
        .retrieve_application(TENANT_A, None, &api_application.id)
        .await
        .expect("tenant admin must retrieve an owner-scoped application");
    let tenant_b_site = repository
        .create_application(
            TENANT_B,
            None,
            None,
            &CreateApplicationRequest {
                name: "Tenant B".to_owned(),
                slug: Some("alpha-0".to_owned()),
                description: None,
                app_kind: "WEB".to_owned(),
                runtime_config: None,
                store_listing: None,
            },
        )
        .await
        .expect("same slug is valid in another tenant");
    assert_ne!(sites[0].id, tenant_b_site.id);

    let duplicate_slug = repository
        .create_application(
            TENANT_A,
            None,
            None,
            &CreateApplicationRequest {
                name: "Duplicate".to_owned(),
                slug: Some("alpha-0".to_owned()),
                description: None,
                app_kind: "WEB".to_owned(),
                runtime_config: None,
                store_listing: None,
            },
        )
        .await
        .expect_err("same tenant slug must conflict");
    assert_eq!(duplicate_slug.kind(), WebServiceErrorKind::Conflict);

    repository
        .retrieve_application(TENANT_A, None, &tenant_b_site.id)
        .await
        .expect_err("tenant A must not retrieve tenant B site");
    repository
        .retrieve_application(TENANT_B, None, &sites[0].id)
        .await
        .expect_err("tenant B must not retrieve tenant A site");

    let tie_sql = "UPDATE web_site SET updated_at = CAST($1 AS TIMESTAMPTZ) WHERE tenant_id = $2";
    sqlx::query(tie_sql)
        .bind("2026-01-01T00:00:00.000Z")
        .bind(TENANT_A)
        .execute(&context.pool)
        .await
        .expect("create deterministic pagination ties");
    let query = ListApplicationsQuery {
        page: 1,
        page_size: 2,
        status: Some(0),
        application_type: Some("WEB".to_owned()),
        site_type: Some(1),
        keyword: Some(" alpha ".to_owned()),
    };
    let first_page = repository
        .list_applications(TENANT_A, None, &query)
        .await
        .expect("list first filtered page");
    let second_page = repository
        .list_applications(
            TENANT_A,
            None,
            &ListApplicationsQuery {
                page: 2,
                ..query.clone()
            },
        )
        .await
        .expect("list second filtered page");
    assert_eq!(first_page.total, 4);
    assert_eq!(first_page.items.len(), 2);
    assert_eq!(second_page.items.len(), 2);
    let first_ids: HashSet<_> = first_page.items.iter().map(|site| &site.id).collect();
    assert!(
        second_page
            .items
            .iter()
            .all(|site| !first_ids.contains(&site.id)),
        "stable pages must not overlap"
    );
    let expected: Vec<String> = sqlx::query(
        "SELECT uuid FROM web_site
         WHERE tenant_id = $1 AND application_type = 'WEB'
         ORDER BY updated_at DESC, id DESC",
    )
    .bind(TENANT_A)
    .fetch_all(&context.pool)
    .await
    .expect("load expected stable ordering")
    .into_iter()
    .map(|row| row.try_get("uuid").expect("site uuid"))
    .collect();
    let observed: Vec<_> = first_page
        .items
        .iter()
        .chain(&second_page.items)
        .map(|site| site.id.clone())
        .collect();
    assert_eq!(observed, expected);

    let deep_page = repository
        .list_applications(
            TENANT_A,
            None,
            &ListApplicationsQuery {
                page: i32::MAX,
                page_size: i32::MAX,
                status: None,
                application_type: None,
                site_type: None,
                keyword: None,
            },
        )
        .await
        .expect_err("invalid page_size must be rejected");
    assert!(matches!(
        deep_page,
        sdkwork_webserver_contract::WebServiceError::Validation(_)
    ));

    verify_source_version_contract(context, &sites[0].id, &sites[1].id, &tenant_b_site.id).await;
    verify_deployment_idempotency(context, &sites[0].id, &sites[1].id).await;
    verify_rollback_atomicity(context, &sites[0].id).await;
    verify_root_domain_zone_contract(context, &sites[0].id).await;
    verify_bounded_config_collections(context, &sites[2].id, &sites[3].id).await;
    verify_public_repository_surface(context, &sites[0].id).await;
}

async fn verify_root_domain_zone_contract(context: &TestContext, site_id: &str) {
    let repository = &context.repository;
    let root_domain = repository
        .create_root_domain(
            TENANT_A,
            &CreateRootDomainRequest {
                hostname: "zone-contract.example".to_string(),
            },
        )
        .await
        .expect("create root-domain Zone");
    assert_eq!(root_domain.subdomain_count, 0);
    assert!(repository
        .retrieve_root_domain(TENANT_B, &root_domain.id)
        .await
        .is_err());

    let duplicate = repository
        .create_root_domain(
            TENANT_A,
            &CreateRootDomainRequest {
                hostname: "zone-contract.example".to_string(),
            },
        )
        .await
        .expect_err("duplicate tenant root-domain Zone must conflict");
    assert_eq!(duplicate.kind(), WebServiceErrorKind::Conflict);

    let apex = repository
        .create_root_domain_hostname(
            TENANT_A,
            &root_domain.id,
            &CreateRootDomainHostnameRequest {
                record_name: "@".to_string(),
                application_id: Some(site_id.to_string()),
                is_primary: true,
                ssl_enabled: true,
                ssl_provider: Some("letsencrypt".to_string()),
            },
        )
        .await
        .expect("create root-domain apex hostname");
    assert_eq!(apex.hostname, "zone-contract.example");
    assert_eq!(apex.record_name.as_deref(), Some("@"));
    assert_eq!(
        apex.root_domain_id.as_deref(),
        Some(root_domain.id.as_str())
    );

    let www = repository
        .create_root_domain_hostname(
            TENANT_A,
            &root_domain.id,
            &CreateRootDomainHostnameRequest {
                record_name: "www".to_string(),
                application_id: None,
                is_primary: false,
                ssl_enabled: true,
                ssl_provider: Some("letsencrypt".to_string()),
            },
        )
        .await
        .expect("create unbound root-domain hostname");
    assert_eq!(www.hostname, "www.zone-contract.example");

    verify_managed_domain_with_evidence(repository, TENANT_A, &apex.id).await;
    let deployment = repository
        .create_deployment(
            TENANT_A,
            site_id,
            Some(91),
            &CreateDeploymentRequest {
                deploy_type: 1,
                environment: Some("production".to_string()),
                version_tag: Some("zone-v1".to_string()),
                idempotency_key: Some("zone-contract-deployment".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("create root-domain projected deployment");
    let deployment_time_sql =
        "UPDATE web_deployment SET status = 2, completed_at = CAST($1 AS TIMESTAMPTZ) WHERE tenant_id = $2 AND uuid = $3";
    sqlx::query(deployment_time_sql)
        .bind("2026-07-30T12:00:00.000Z")
        .bind(TENANT_A)
        .bind(&deployment.id)
        .execute(&context.pool)
        .await
        .expect("mark projected deployment successful");

    let hostnames = repository
        .list_root_domain_hostnames(TENANT_A, &root_domain.id, 1, 20)
        .await
        .expect("list root-domain hostnames");
    assert_eq!(hostnames.total, 2);
    let refreshed_apex = hostnames
        .items
        .iter()
        .find(|item| item.id == apex.id)
        .expect("find projected apex hostname");
    assert!(refreshed_apex.is_verified);
    assert_eq!(
        refreshed_apex
            .latest_deployment
            .as_ref()
            .map(|item| item.id.as_str()),
        Some(deployment.id.as_str())
    );

    let page = repository
        .list_root_domains(
            TENANT_A,
            &ListRootDomainsQuery {
                page: 1,
                page_size: 20,
                status: Some(1),
                keyword: Some("ZONE-CONTRACT".to_string()),
            },
        )
        .await
        .expect("filter root-domain Zones");
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].subdomain_count, 2);
    assert_eq!(page.items[0].bound_subdomain_count, 1);
    assert_eq!(page.items[0].verified_subdomain_count, 1);
    assert_eq!(page.items[0].https_subdomain_count, 0);
    assert_eq!(page.items[0].active_deployment_count, 1);

    let non_empty_delete = repository
        .delete_root_domain(TENANT_A, &root_domain.id)
        .await
        .expect_err("non-empty root-domain Zone must not be deleted");
    assert_eq!(non_empty_delete.kind(), WebServiceErrorKind::Conflict);

    repository
        .unbind_managed_domain(TENANT_A, &apex.id)
        .await
        .expect("unbind root-domain apex hostname");
    repository
        .delete_managed_domain(TENANT_A, &apex.id)
        .await
        .expect("delete unbound root-domain apex hostname");
    repository
        .delete_managed_domain(TENANT_A, &www.id)
        .await
        .expect("delete unbound root-domain www hostname");
    repository
        .delete_root_domain(TENANT_A, &root_domain.id)
        .await
        .expect("delete empty root-domain Zone");
}

async fn verify_bounded_config_collections(
    context: &TestContext,
    env_site_id: &str,
    health_site_id: &str,
) {
    let repository = &context.repository;
    for index in 0..99 {
        repository
            .create_env_variable(
                TENANT_A,
                env_site_id,
                &CreateEnvVariableRequest {
                    key: format!("CAPACITY_ENV_{index:03}"),
                    value: "bounded".to_string(),
                    environment: "production".to_string(),
                    is_secret: false,
                },
            )
            .await
            .expect("seed bounded environment-variable collection");
    }

    let first_env = CreateEnvVariableRequest {
        key: "CAPACITY_ENV_FINAL_A".to_string(),
        value: "bounded".to_string(),
        environment: "production".to_string(),
        is_secret: false,
    };
    let second_env = CreateEnvVariableRequest {
        key: "CAPACITY_ENV_FINAL_B".to_string(),
        ..first_env.clone()
    };
    let (first_result, second_result) = tokio::join!(
        repository.create_env_variable(TENANT_A, env_site_id, &first_env),
        repository.create_env_variable(TENANT_A, env_site_id, &second_env),
    );
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1,
        "the site lock must serialize concurrent environment-variable capacity checks"
    );
    for result in [first_result, second_result] {
        if let Err(error) = result {
            assert_eq!(error.kind(), WebServiceErrorKind::Conflict);
        }
    }
    let env_page = repository
        .list_env_variables(TENANT_A, env_site_id, None)
        .await
        .expect("list the maximum bounded environment-variable collection");
    assert_eq!(env_page.total, 100);
    assert_eq!(env_page.items.len(), 100);

    for index in 0..99 {
        repository
            .create_health_check(
                TENANT_A,
                health_site_id,
                &CreateHealthCheckRequest {
                    check_type: 1,
                    check_url: format!("https://health-{index:03}.example.test/ready"),
                    check_interval: 60,
                    timeout_ms: 5_000,
                    retry_count: 3,
                },
            )
            .await
            .expect("seed bounded health-check collection");
    }

    let first_health = CreateHealthCheckRequest {
        check_type: 1,
        check_url: "https://health-final-a.example.test/ready".to_string(),
        check_interval: 60,
        timeout_ms: 5_000,
        retry_count: 3,
    };
    let second_health = CreateHealthCheckRequest {
        check_type: 1,
        check_url: "https://health-final-b.example.test/ready".to_string(),
        ..first_health.clone()
    };
    let (first_result, second_result) = tokio::join!(
        repository.create_health_check(TENANT_A, health_site_id, &first_health),
        repository.create_health_check(TENANT_A, health_site_id, &second_health),
    );
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1,
        "the site lock must serialize concurrent health-check capacity checks"
    );
    for result in [first_result, second_result] {
        if let Err(error) = result {
            assert_eq!(error.kind(), WebServiceErrorKind::Conflict);
        }
    }
    let health_page = repository
        .list_health_checks(TENANT_A, health_site_id)
        .await
        .expect("list the maximum bounded health-check collection");
    assert_eq!(health_page.total, 100);
    assert_eq!(health_page.items.len(), 100);
}

async fn verify_public_repository_surface(context: &TestContext, site_id: &str) {
    let repository = &context.repository;
    let metadata_expression = "CAST($3 AS JSONB)";
    let statement = format!(
        "UPDATE web_site SET metadata = {metadata_expression} WHERE tenant_id = $1 AND uuid = $2"
    );
    sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
        .bind(TENANT_A)
        .bind(site_id)
        .bind(r#"{"system":{"retention":"managed"}}"#)
        .execute(&context.pool)
        .await
        .expect("seed unrelated site metadata");
    let updated_site = repository
        .update_application(
            TENANT_A,
            site_id,
            &UpdateApplicationRequest {
                name: Some("Alpha Site Updated".to_string()),
                description: Some("dual-engine repository parity".to_string()),
                runtime_config: Some(serde_json::json!({
                    "workers": 4,
                    "features": {"http2": true, "https": true}
                })),
                store_listing: Some(ApplicationStoreListing {
                    icon: Some(test_media_resource("icon-node", 1024, 1024)),
                    short_description: Some("Production-ready application".to_string()),
                    ..ApplicationStoreListing::default()
                }),
            },
        )
        .await
        .expect("update site JSON and timestamp fields");
    assert_eq!(updated_site.name, "Alpha Site Updated");
    assert_eq!(
        updated_site
            .store_listing
            .as_ref()
            .and_then(|listing| listing.icon.as_ref())
            .and_then(|icon| icon.id.as_deref()),
        Some("icon-node")
    );
    let metadata: String = sqlx::query_scalar(
        "SELECT CAST(metadata AS TEXT) FROM web_site WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(site_id)
    .fetch_one(&context.pool)
    .await
    .expect("read merged site metadata");
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata).expect("parse merged metadata");
    assert_eq!(
        metadata
            .pointer("/system/retention")
            .and_then(serde_json::Value::as_str),
        Some("managed")
    );
    assert_eq!(
        metadata
            .pointer("/storeListing/icon/id")
            .and_then(serde_json::Value::as_str),
        Some("icon-node")
    );
    assert_eq!(
        updated_site
            .runtime_config
            .as_ref()
            .and_then(|value| value.pointer("/features/https"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        repository
            .set_application_status(TENANT_A, site_id, 1)
            .await
            .expect("update site status timestamp")
            .status,
        1
    );

    repository
        .create_root_domain(
            TENANT_A,
            &CreateRootDomainRequest {
                hostname: "example.test".to_string(),
            },
        )
        .await
        .expect("define root domain before creating hostnames");

    let domain = repository
        .create_domain(
            TENANT_A,
            site_id,
            &CreateDomainRequest {
                hostname: "parity.example.test".to_string(),
                is_primary: true,
                ssl_enabled: true,
                ssl_provider: Some("acme".to_string()),
            },
        )
        .await
        .expect("create domain with transactional primary update");
    assert_eq!(
        repository
            .retrieve_domain(TENANT_A, site_id, &domain.id)
            .await
            .expect("retrieve domain")
            .hostname,
        "parity.example.test"
    );
    assert_eq!(
        repository
            .list_domains(TENANT_A, site_id, 1, 20)
            .await
            .expect("list domains")
            .total,
        1
    );
    assert_eq!(
        verify_site_domain_with_evidence(repository, TENANT_A, site_id, &domain.id)
            .await
            .status,
        "VERIFIED"
    );

    let detached_domain = repository
        .create_managed_domain(
            TENANT_A,
            &CreateManagedDomainRequest {
                hostname: "detached.example.test".to_string(),
                application_id: None,
                is_primary: false,
                ssl_enabled: true,
                ssl_provider: Some("letsencrypt".to_string()),
            },
        )
        .await
        .expect("create detached managed domain");
    assert!(detached_domain.application_id.is_none());
    assert_eq!(detached_domain.certificate_count, 0);
    assert_eq!(
        verify_managed_domain_with_evidence(repository, TENANT_A, &detached_domain.id)
            .await
            .status,
        "VERIFIED"
    );
    let self_signed_issue_auto_renew_error = repository
        .enqueue_certificate_issue(
            TENANT_A,
            None,
            Some(91),
            &IssueCertificateRequest {
                domain_ids: vec![detached_domain.id.clone()],
                cert_type: 3,
                key_algorithm: "ECDSA".to_string(),
                auto_renew: true,
            },
            Some("detached-self-signed-auto-renew"),
        )
        .await
        .expect_err("self-signed issue must reject automatic renewal at the repository boundary");
    assert_eq!(
        self_signed_issue_auto_renew_error.kind(),
        WebServiceErrorKind::Validation
    );
    assert_eq!(
        repository
            .list_managed_domains(TENANT_A, 1, 20)
            .await
            .expect("list tenant managed domains")
            .total,
        2
    );
    assert_eq!(
        repository
            .list_managed_domains(TENANT_B, 1, 20)
            .await
            .expect("list other tenant managed domains")
            .total,
        0
    );

    let detached_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        None,
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![detached_domain.id.clone()],
            cert_type: 3,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: false,
        },
        "detached-certificate-ecdsa",
        "repository-detached-ecdsa",
    )
    .await;
    let detached_certificate = repository
        .finalize_certificate_operation(
            &detached_lease,
            &test_certificate_update("detached.example.test", 3, "ECDSA", '1', false),
        )
        .await
        .expect("finalize certificate for detached domain");
    let self_signed_auto_renew_error = repository
        .update_certificate_auto_renew(TENANT_A, &detached_certificate.id, true)
        .await
        .expect_err("self-signed certificates must not enable automatic renewal");
    assert_eq!(
        self_signed_auto_renew_error.kind(),
        WebServiceErrorKind::Validation
    );
    assert_eq!(
        detached_certificate.identifiers[0].domain_id.as_str(),
        detached_domain.id.as_str()
    );
    let replacement_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        None,
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![detached_domain.id.clone()],
            cert_type: 3,
            key_algorithm: "RSA".to_string(),
            auto_renew: false,
        },
        "detached-certificate-rsa",
        "repository-detached-rsa",
    )
    .await;
    let replacement_certificate = repository
        .finalize_certificate_operation(
            &replacement_lease,
            &test_certificate_update("detached.example.test", 3, "RSA", '2', false),
        )
        .await
        .expect("finalize a second certificate for the detached domain");
    assert_ne!(replacement_certificate.id, detached_certificate.id);
    assert_eq!(
        repository
            .list_managed_domains(TENANT_A, 1, 20)
            .await
            .expect("refresh tenant managed domains")
            .items
            .into_iter()
            .find(|item| item.id == detached_domain.id)
            .expect("find detached managed domain")
            .certificate_count,
        2
    );
    assert!(repository
        .list_certificates(TENANT_A, None, None, None, 1, 20)
        .await
        .expect("list detached certificate for backend admin")
        .items
        .iter()
        .any(|item| item.id == detached_certificate.id));
    assert!(!repository
        .list_certificates(TENANT_A, Some(91), None, None, 1, 20)
        .await
        .expect("list owner-scoped certificates")
        .items
        .iter()
        .any(|item| item.id == detached_certificate.id));

    let bound_domain = repository
        .bind_managed_domain(
            TENANT_A,
            &detached_domain.id,
            &UpdateDomainApplicationBindingRequest {
                application_id: site_id.to_string(),
                is_primary: false,
            },
        )
        .await
        .expect("bind detached domain to application");
    assert_eq!(bound_domain.application_id.as_deref(), Some(site_id));
    assert_eq!(bound_domain.certificate_count, 2);
    let active_binding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_site_binding b
         INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
         WHERE b.tenant_id = $1 AND d.uuid = $2 AND b.status <> 'ARCHIVED'
           AND b.deleted_at IS NULL",
    )
    .bind(TENANT_A)
    .bind(&detached_domain.id)
    .fetch_one(&context.pool)
    .await
    .expect("read active domain application binding");
    assert_eq!(active_binding_count, 1);

    let unbound_domain = repository
        .unbind_managed_domain(TENANT_A, &detached_domain.id)
        .await
        .expect("unbind managed domain");
    assert!(unbound_domain.application_id.is_none());
    let archived_binding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_site_binding b
         INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
         WHERE b.tenant_id = $1 AND d.uuid = $2 AND b.status = 'ARCHIVED'
           AND b.deleted_at IS NOT NULL",
    )
    .bind(TENANT_A)
    .bind(&detached_domain.id)
    .fetch_one(&context.pool)
    .await
    .expect("read archived domain application binding");
    assert_eq!(archived_binding_count, 1);
    assert_eq!(
        repository
            .delete_domain(TENANT_A, site_id, &detached_domain.id)
            .await
            .expect_err("application scope must not mutate an unbound domain")
            .kind(),
        WebServiceErrorKind::NotFound
    );
    assert_eq!(
        repository
            .delete_managed_domain(TENANT_A, &detached_domain.id)
            .await
            .expect_err("domain with certificates must not be deleted")
            .kind(),
        WebServiceErrorKind::Conflict
    );

    let disposable_domain = repository
        .create_managed_domain(
            TENANT_A,
            &CreateManagedDomainRequest {
                hostname: "disposable.example.test".to_string(),
                application_id: None,
                is_primary: false,
                ssl_enabled: false,
                ssl_provider: Some("none".to_string()),
            },
        )
        .await
        .expect("create disposable managed domain");
    repository
        .delete_managed_domain(TENANT_A, &disposable_domain.id)
        .await
        .expect("delete unbound domain without certificates");

    let public_env = repository
        .create_env_variable(
            TENANT_A,
            site_id,
            &CreateEnvVariableRequest {
                key: "PUBLIC_MODE".to_string(),
                value: "strict".to_string(),
                environment: "production".to_string(),
                is_secret: false,
            },
        )
        .await
        .expect("create public environment variable");
    assert_eq!(public_env.value, "strict");
    let secret_env = repository
        .create_env_variable(
            TENANT_A,
            site_id,
            &CreateEnvVariableRequest {
                key: "PRIVATE_TOKEN".to_string(),
                value: "test-only-secret".to_string(),
                environment: "production".to_string(),
                is_secret: true,
            },
        )
        .await
        .expect("create encrypted environment variable");
    assert_eq!(secret_env.value, "***");
    let env_page = repository
        .list_env_variables(TENANT_A, site_id, Some("production"))
        .await
        .expect("list environment variables");
    assert_eq!(env_page.total, 2);
    assert!(env_page
        .items
        .iter()
        .any(|item| item.key == "PRIVATE_TOKEN" && item.value == "***"));

    repository
        .create_health_check(
            TENANT_A,
            site_id,
            &CreateHealthCheckRequest {
                check_type: 1,
                check_url: "https://parity.example.test/healthz".to_string(),
                check_interval: 30,
                timeout_ms: 5_000,
                retry_count: 2,
            },
        )
        .await
        .expect("create health check timestamps");
    assert_eq!(
        repository
            .list_health_checks(TENANT_A, site_id)
            .await
            .expect("list health checks")
            .total,
        1
    );

    let nginx = repository
        .create_nginx_config(
            TENANT_A,
            &CreateNginxConfigRequest {
                site_id: site_id.to_string(),
                config_name: "parity.conf".to_string(),
                config_type: 1,
                config_content: "server { listen 443 ssl; }".to_string(),
            },
        )
        .await
        .expect("create nginx config timestamps");
    let nginx = repository
        .update_nginx_config(
            Some(TENANT_A),
            &nginx.id,
            &UpdateNginxConfigRequest {
                config_name: Some("parity-updated.conf".to_string()),
                config_content: Some("server { listen 443 ssl http2; }".to_string()),
            },
        )
        .await
        .expect("update nginx config timestamp");
    assert_eq!(
        repository
            .list_nginx_configs(
                Some(TENANT_A),
                &ListNginxConfigsQuery {
                    page: 1,
                    page_size: 20,
                    site_id: Some(site_id.to_string()),
                    config_type: Some(1),
                    is_active: None,
                },
            )
            .await
            .expect("list nginx configs")
            .total,
        1
    );
    assert_eq!(
        repository
            .list_nginx_configs(
                None,
                &ListNginxConfigsQuery {
                    page: 1,
                    page_size: 20,
                    site_id: None,
                    config_type: Some(1),
                    is_active: None,
                },
            )
            .await
            .expect("list nginx configs across tenants")
            .total,
        1
    );
    assert_eq!(
        repository
            .list_nginx_configs(
                None,
                &ListNginxConfigsQuery {
                    page: 1,
                    page_size: 20,
                    site_id: Some("missing-site".to_string()),
                    config_type: Some(1),
                    is_active: None,
                },
            )
            .await
            .expect("global nginx config list applies the site filter")
            .total,
        0
    );
    repository
        .web_nginx_config(None, &nginx.id)
        .await
        .expect("atomically activate nginx config through global backend scope");
    verify_nginx_activation_rollback(context, site_id, &nginx.id).await;

    let certificate_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        Some(91),
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![domain.id.clone()],
            cert_type: 1,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: true,
        },
        "parity-certificate-issue",
        "repository-parity-issue",
    )
    .await;
    let certificate_id = certificate_lease.certificate_id.clone();
    let certificate_update = test_certificate_update("parity.example.test", 1, "ECDSA", '5', true);
    let certificate = repository
        .finalize_certificate_operation(&certificate_lease, &certificate_update)
        .await
        .expect("finalize certificate operation and version atomically");
    assert_eq!(certificate.status, "ISSUED");
    let listener_binding = repository
        .bind_listener_certificate(
            TENANT_A,
            site_id,
            &domain.id,
            &CreateListenerCertificateBindingRequest {
                certificate_id: certificate_id.clone(),
                certificate_version_id: None,
                priority: 100,
                is_default: true,
            },
        )
        .await
        .expect("bind the active certificate version to the domain listener");
    assert_eq!(listener_binding.key_algorithm, "ECDSA");
    assert!(listener_binding.is_default);
    assert_eq!(
        listener_binding.desired_certificate.cert_name,
        "parity.example.test"
    );
    assert_eq!(listener_binding.desired_certificate.identifiers.len(), 1);
    assert_eq!(
        listener_binding.desired_certificate.issuer.as_deref(),
        Some("SDKWork Test CA")
    );
    assert_eq!(listener_binding.desired_certificate.status, "ISSUED");
    assert_eq!(
        repository
            .list_listener_certificate_bindings(TENANT_A, site_id, &domain.id, 1, 20)
            .await
            .expect("list domain listener certificates")
            .total,
        1
    );
    let initial_listener_version_id = listener_binding.desired_certificate_version_id.clone();
    assert_eq!(
        repository
            .list_certificates(TENANT_A, None, None, None, 1, 20)
            .await
            .expect("list certificate timestamp projections")
            .total,
        3
    );
    assert_eq!(
        repository
            .list_certificates(TENANT_A, Some(91), Some(site_id), None, 1, 20)
            .await
            .expect("owning user can list application certificates")
            .total,
        1
    );
    assert_eq!(
        repository
            .list_certificates(TENANT_A, None, None, Some(&domain.id), 1, 20)
            .await
            .expect("filter certificates by domain")
            .total,
        1
    );
    assert!(repository
        .list_certificates(TENANT_A, Some(92), Some(site_id), None, 1, 20)
        .await
        .expect("another user receives an empty certificate page")
        .items
        .is_empty());
    repository
        .enqueue_certificate_issue(
            TENANT_A,
            Some(92),
            Some(92),
            &IssueCertificateRequest {
                domain_ids: vec![domain.id.clone()],
                cert_type: 1,
                key_algorithm: "ECDSA".to_string(),
                auto_renew: true,
            },
            Some("parity-wrong-owner-issue"),
        )
        .await
        .expect_err("another user cannot issue a certificate for an owned application domain");
    let disabled_certificate = repository
        .update_certificate_auto_renew(TENANT_A, &certificate_id, false)
        .await
        .expect("disable certificate automatic renewal");
    assert_eq!(disabled_certificate.id, certificate_id);
    assert_eq!(disabled_certificate.auto_renew, Some(false));
    assert_eq!(
        repository
            .schedule_due_certificate_renewals(365, 20)
            .await
            .expect("exclude disabled certificate from renewal schedule"),
        0
    );
    repository
        .enqueue_certificate_renewal(
            TENANT_B,
            &certificate_id,
            Some(91),
            Some("parity-wrong-tenant-renewal"),
        )
        .await
        .expect_err("certificate renewal operation must remain tenant isolated");
    let manual_renewal = repository
        .enqueue_certificate_renewal(
            TENANT_A,
            &certificate_id,
            Some(91),
            Some("parity-manual-renewal-disabled-policy"),
        )
        .await
        .expect("manual renewal is independent of the automatic renewal policy");
    let manual_lease = repository
        .claim_certificate_operations("repository-manual-renewal", 60, 32)
        .await
        .expect("claim manual renewal")
        .into_iter()
        .find(|lease| lease.operation_id == manual_renewal.operation_id)
        .expect("manual renewal must be claimable");
    assert!(!manual_lease.auto_renew);
    repository
        .finalize_certificate_operation(
            &manual_lease,
            &test_certificate_update("parity.example.test", 1, "ECDSA", '6', false),
        )
        .await
        .expect("finalize manual renewal while automatic renewal is disabled");
    let manual_renewal_replay = repository
        .enqueue_certificate_renewal(
            TENANT_A,
            &certificate_id,
            Some(91),
            Some("parity-manual-renewal-disabled-policy"),
        )
        .await
        .expect("replay completed renewal with the original idempotency key");
    assert_eq!(
        manual_renewal_replay.operation_id,
        manual_renewal.operation_id
    );
    assert_eq!(manual_renewal_replay.status, "SUCCEEDED");
    let enabled_certificate = repository
        .update_certificate_auto_renew(TENANT_A, &certificate_id, true)
        .await
        .expect("enable certificate automatic renewal");
    assert_eq!(enabled_certificate.id, certificate_id);
    assert_eq!(
        repository
            .list_certificates(TENANT_A, None, None, None, 1, 20)
            .await
            .expect("automatic renewal update preserves canonical row")
            .total,
        3
    );
    let same_algorithm_lease = enqueue_and_claim_certificate(
        repository,
        TENANT_A,
        Some(91),
        Some(91),
        &IssueCertificateRequest {
            domain_ids: vec![domain.id.clone()],
            cert_type: 3,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: false,
        },
        "parity-listener-same-algorithm",
        "repository-listener-same-algorithm",
    )
    .await;
    let same_algorithm_certificate = repository
        .finalize_certificate_operation(
            &same_algorithm_lease,
            &test_certificate_update("parity.example.test", 3, "ECDSA", 'e', false),
        )
        .await
        .expect("finalize a second ECDSA certificate for the listener domain");
    let same_algorithm_error = repository
        .bind_listener_certificate(
            TENANT_A,
            site_id,
            &domain.id,
            &CreateListenerCertificateBindingRequest {
                certificate_id: same_algorithm_certificate.id,
                certificate_version_id: None,
                priority: 200,
                is_default: false,
            },
        )
        .await
        .expect_err("one listener cannot bind two active ECDSA certificates");
    assert_eq!(same_algorithm_error.kind(), WebServiceErrorKind::Conflict);
    assert!(same_algorithm_error
        .to_string()
        .contains("key algorithm ECDSA"));
    assert_eq!(
        repository
            .schedule_due_certificate_renewals(365, 20)
            .await
            .expect("schedule due certificate renewal"),
        1
    );
    repository
        .update_certificate_auto_renew(TENANT_A, &certificate_id, false)
        .await
        .expect_err("automatic renewal policy cannot invalidate a pending operation");
    let first_renewal_lease = repository
        .claim_certificate_operations("repository-scheduled-renewal-a", 60, 32)
        .await
        .expect("claim scheduled certificate renewal")
        .into_iter()
        .find(|lease| lease.certificate_id == certificate_id)
        .expect("scheduled certificate renewal must be claimable");
    assert_eq!(first_renewal_lease.operation_type, "RENEW");
    assert!(repository
        .claim_certificate_operations("repository-scheduled-renewal-b", 60, 32)
        .await
        .expect("active operation is not claimable before lease expiry")
        .is_empty());
    repository
        .update_certificate_auto_renew(TENANT_A, &certificate_id, false)
        .await
        .expect_err("automatic renewal policy cannot invalidate an active claim");

    sqlx::query(
        "UPDATE web_certificate_operation SET lease_expires_at = NOW() - INTERVAL '1 second'
         WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&first_renewal_lease.operation_id)
    .execute(&context.pool)
    .await
    .expect("expire scheduled certificate operation lease");
    let replacement_renewal_lease = repository
        .claim_certificate_operations("repository-scheduled-renewal-b", 60, 32)
        .await
        .expect("reclaim stale certificate operation")
        .into_iter()
        .find(|lease| lease.operation_id == first_renewal_lease.operation_id)
        .expect("expired certificate operation must be reclaimed");
    assert!(replacement_renewal_lease.fencing_token > first_renewal_lease.fencing_token);
    assert!(replacement_renewal_lease.attempt_count > first_renewal_lease.attempt_count);
    let renewed_certificate_update =
        test_certificate_update("parity.example.test", 1, "ECDSA", '7', true);

    let stale_finalize = repository
        .finalize_certificate_operation(&first_renewal_lease, &renewed_certificate_update)
        .await
        .expect_err("stale renewal worker must not finalize over a replacement claim");
    assert_eq!(stale_finalize.kind(), WebServiceErrorKind::Conflict);
    let stale_failure = repository
        .fail_certificate_operation(
            &first_renewal_lease,
            "STALE_WORKER_FAILURE",
            "2099-01-01T00:00:00Z",
            "2099-01-02T00:00:00Z",
        )
        .await
        .expect_err("stale renewal worker must not fail a replacement claim");
    assert_eq!(stale_failure.kind(), WebServiceErrorKind::Conflict);

    repository
        .finalize_certificate_operation(&replacement_renewal_lease, &renewed_certificate_update)
        .await
        .expect("current renewal claim finalizes successfully");
    let rotated_listener = repository
        .list_listener_certificate_bindings(TENANT_A, site_id, &domain.id, 1, 20)
        .await
        .expect("read listener binding after renewal")
        .items
        .into_iter()
        .next()
        .expect("listener binding remains active after renewal");
    assert_ne!(
        rotated_listener.desired_certificate_version_id,
        initial_listener_version_id
    );
    let exhausted_operation = repository
        .enqueue_certificate_renewal(
            TENANT_A,
            &certificate_id,
            Some(91),
            Some("parity-exhausted-renewal"),
        )
        .await
        .expect("enqueue renewal for exhausted lease recovery");
    let mut exhausted_lease = repository
        .claim_certificate_operations("repository-exhausted-renewal", 60, 32)
        .await
        .expect("claim renewal for exhausted lease recovery")
        .into_iter()
        .find(|lease| lease.operation_id == exhausted_operation.operation_id)
        .expect("exhausted renewal must be claimable");
    for _ in 1..exhausted_lease.max_attempts {
        let operation = repository
            .fail_certificate_operation(
                &exhausted_lease,
                "SYNTHETIC_RETRYABLE_FAILURE",
                "2000-01-01T00:00:00Z",
                "2099-01-02T00:00:00Z",
            )
            .await
            .expect("persist retryable certificate operation failure");
        assert_eq!(operation.status, "PENDING");
        exhausted_lease = repository
            .claim_certificate_operations("repository-exhausted-renewal", 60, 32)
            .await
            .expect("reclaim retryable certificate operation")
            .into_iter()
            .find(|lease| lease.operation_id == exhausted_operation.operation_id)
            .expect("retryable certificate operation must be claimable");
    }
    assert_eq!(exhausted_lease.attempt_count, exhausted_lease.max_attempts);
    sqlx::query(
        "UPDATE web_certificate_operation SET lease_expires_at = NOW() - INTERVAL '1 second'
         WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&exhausted_operation.operation_id)
    .execute(&context.pool)
    .await
    .expect("expire retry-exhausted certificate operation lease");
    assert!(repository
        .claim_certificate_operations("repository-exhausted-reaper", 60, 32)
        .await
        .expect("reap retry-exhausted certificate operation")
        .is_empty());
    let exhausted_status = repository
        .retrieve_certificate_operation(TENANT_A, None, &exhausted_operation.operation_id)
        .await
        .expect("retrieve reaped certificate operation");
    assert_eq!(exhausted_status.status, "FAILED");
    assert_eq!(
        exhausted_status.failure_code.as_deref(),
        Some("CERTIFICATE_OPERATION_LEASE_EXPIRED")
    );

    let failed_issue = repository
        .enqueue_certificate_issue(
            TENANT_A,
            None,
            Some(91),
            &IssueCertificateRequest {
                domain_ids: vec![domain.id.clone()],
                cert_type: 1,
                key_algorithm: "ECDSA".to_string(),
                auto_renew: false,
            },
            Some("parity-terminal-issuance-failure"),
        )
        .await
        .expect("enqueue certificate for terminal issuance failure");
    sqlx::query(
        "UPDATE web_certificate_operation SET max_attempts = 1 WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&failed_issue.operation_id)
    .execute(&context.pool)
    .await
    .expect("bound failed issuance retry budget");
    let failed_issue_lease = repository
        .claim_certificate_operations("repository-failed-issuance", 60, 32)
        .await
        .expect("claim failed issuance operation")
        .into_iter()
        .find(|lease| lease.operation_id == failed_issue.operation_id)
        .expect("failed issuance operation must be claimable");
    let failed_issue_status = repository
        .fail_certificate_operation(
            &failed_issue_lease,
            "SYNTHETIC_ISSUANCE_FAILURE",
            "2099-01-01T00:00:00Z",
            "2099-01-02T00:00:00Z",
        )
        .await
        .expect("write terminal certificate issuance failure");
    assert_eq!(failed_issue_status.status, "FAILED");

    let server = repository
        .create_server(
            TENANT_A,
            &CreateServerRequest {
                name: "Parity Edge".to_string(),
                host: "192.0.2.44".to_string(),
                tenant_scope_hash: "a".repeat(64),
                ssh_port: 22,
            },
        )
        .await
        .expect("create server JSON and timestamps");
    let authenticated = repository
        .authenticate_agent_token(&server.agent_token)
        .await
        .expect("authenticate agent token from JSON metadata");
    assert_eq!(authenticated, (server.server.id.clone(), TENANT_A));
    verify_runtime_assignment_contract(context, &server.server.id, site_id).await;
    repository
        .record_agent_heartbeat(
            &server.server.id,
            TENANT_A,
            &AgentHeartbeatRequest {
                agent_version: Some("1.0.0-test".to_string()),
                nginx_enabled: Some(true),
                active_configs: Some(1),
                last_sync_version: None,
                certificate_observations: Vec::new(),
            },
        )
        .await
        .expect("merge heartbeat JSON and timestamp");
    assert!(repository
        .list_servers(TENANT_A, 1, 20, None)
        .await
        .expect("list server JSON and timestamp projections")
        .items
        .iter()
        .any(|item| item.id == server.server.id && item.last_heartbeat_at.is_some()));
    let sync = repository
        .build_agent_sync_manifest(&server.server.id, TENANT_A, None)
        .await
        .expect("build agent sync JSON projections");
    assert_eq!(sync.nginx_configs.len(), 1);
    assert_eq!(
        sync.certificates.len(),
        1,
        "an active listener binding must distribute its exact certificate version"
    );
    assert!(sync.certificates[0]
        .fullchain_pem
        .contains("BEGIN CERTIFICATE"));
    assert!(sync.certificates[0]
        .privkey_pem
        .contains("BEGIN PRIVATE KEY"));
    let pending_distribution = repository
        .list_certificate_distribution(TENANT_A, 1, 20)
        .await
        .expect("list pending certificate distribution");
    let pending_server = pending_distribution
        .items
        .iter()
        .find(|item| item.server_id == server.server.id)
        .expect("pending server distribution item");
    assert_eq!(pending_server.status, "PENDING");
    assert_eq!(pending_server.desired_sync_version, sync.sync_version);

    repository
        .record_agent_heartbeat(
            &server.server.id,
            TENANT_A,
            &AgentHeartbeatRequest {
                agent_version: Some("1.0.0-test".to_string()),
                nginx_enabled: Some(true),
                active_configs: Some(1),
                last_sync_version: Some(sync.sync_version.clone()),
                certificate_observations: vec![AgentCertificateObservation {
                    certificate_id: sync.certificates[0].certificate_id.clone(),
                    fingerprint: sync.certificates[0].fingerprint.clone(),
                    sync_version: sync.sync_version.clone(),
                    state: "SERVED".to_string(),
                    observed_at: chrono::Utc::now().to_rfc3339(),
                    failure_code: None,
                }],
            },
        )
        .await
        .expect("report applied certificate sync version");
    let offline_server = repository
        .create_server(
            TENANT_A,
            &CreateServerRequest {
                name: "Parity Offline Edge".to_string(),
                host: "192.0.2.45".to_string(),
                tenant_scope_hash: "a".repeat(64),
                ssh_port: 22,
            },
        )
        .await
        .expect("create offline distribution server");
    let converged_distribution = repository
        .list_certificate_distribution(TENANT_A, 1, 20)
        .await
        .expect("list converged certificate distribution");
    assert_eq!(converged_distribution.total, 2);
    assert!(converged_distribution.items.iter().any(|item| {
        item.server_id == server.server.id
            && item.status == "SYNCED"
            && item.applied_sync_version.as_deref() == Some(sync.sync_version.as_str())
    }));
    assert!(converged_distribution
        .items
        .iter()
        .any(|item| item.server_id == offline_server.server.id && item.status == "OFFLINE"));
    assert_eq!(
        repository
            .build_agent_sync_manifest(&offline_server.server.id, TENANT_A, None)
            .await
            .expect_err("an unassigned node must not receive tenant-wide material")
            .kind(),
        WebServiceErrorKind::Conflict
    );
    verify_node_sync_database_bounds(context, &server.server.id, &nginx.id, &certificate_id).await;

    repository
        .insert_audit_log(AuditLogWrite {
            tenant_id: TENANT_A,
            organization_id: 31,
            operator_id: 91,
            operator_type: "USER",
            action: "repository.parity",
            target_type: "site",
            target_id: None,
            target_uuid: Some(site_id),
            request_id: Some("request-parity-1"),
            metadata_json: "{\"source\":\"repository-parity\"}",
        })
        .await
        .expect("insert audit timestamp");
    // Audit logs are a growing collection and list only supports opaque
    // keyset cursor pagination (PAGINATION_SPEC §6/§12): no OFFSET, and
    // `total` is not computed in cursor mode. A first-page cursor is the
    // far-future bound `(9999-12-31T23:59:59Z, i64::MAX)` encoded as
    // base64url of `v1|<created_at>|<id>`.
    let audit_page = repository
        .list_audit_logs(
            Some(TENANT_A),
            &ListAuditLogsQuery {
                cursor: Some(
                    "djF8OTk5OS0xMi0zMVQyMzo1OTo1OVp8OTIyMzM3MjAzNjg1NDc3NTgwNw".to_string(),
                ),
                page_size: Some(20),
                ..ListAuditLogsQuery::default()
            },
        )
        .await
        .expect("list audit timestamp projections");
    assert_eq!(audit_page.items[0].action, "repository.parity");
    assert_eq!(audit_page.has_more, Some(false));

    let first_audit_page = repository
        .list_audit_logs(
            Some(TENANT_A),
            &ListAuditLogsQuery {
                page_size: Some(20),
                ..ListAuditLogsQuery::default()
            },
        )
        .await
        .expect("list audit logs without cursor on first page");
    assert_eq!(first_audit_page.items[0].action, "repository.parity");

    repository
        .unbind_listener_certificate(TENANT_A, site_id, &domain.id, &listener_binding.id)
        .await
        .expect("unbind domain listener certificate");
    assert_eq!(
        repository
            .list_listener_certificate_bindings(TENANT_A, site_id, &domain.id, 1, 20)
            .await
            .expect("list domain listener certificates after unbind")
            .total,
        0
    );
    let rebound_listener = repository
        .bind_listener_certificate(
            TENANT_A,
            site_id,
            &domain.id,
            &CreateListenerCertificateBindingRequest {
                certificate_id: certificate_id.clone(),
                certificate_version_id: Some(initial_listener_version_id.clone()),
                priority: 100,
                is_default: true,
            },
        )
        .await
        .expect("rebind a still-valid superseded certificate version");
    assert_eq!(rebound_listener.status, "PENDING");
    assert_eq!(
        rebound_listener.desired_certificate_version_id,
        initial_listener_version_id
    );
    assert!(rebound_listener.current_certificate_version_id.is_none());
    assert!(rebound_listener.current_certificate.is_none());
    assert!(rebound_listener.activated_at.is_none());
    repository
        .unbind_listener_certificate(TENANT_A, site_id, &domain.id, &rebound_listener.id)
        .await
        .expect("remove rebound listener certificate before deleting the domain");

    repository
        .delete_domain(TENANT_A, site_id, &domain.id)
        .await
        .expect("soft-delete domain timestamps");
    assert!(repository
        .list_certificates(TENANT_A, Some(91), None, Some(&domain.id), 1, 20)
        .await
        .expect("owner certificate remains visible after application unbind")
        .items
        .iter()
        .any(|item| item.id == certificate_id));
    assert!(repository
        .list_certificates(TENANT_A, Some(92), None, Some(&domain.id), 1, 20)
        .await
        .expect("another owner cannot list the detached certificate")
        .items
        .is_empty());
    let active_delete = repository
        .delete_application(TENANT_A, site_id, Some(91))
        .await
        .expect_err("active site deletion must be rejected");
    assert_eq!(active_delete.kind(), WebServiceErrorKind::Conflict);
    repository
        .set_application_status(TENANT_A, site_id, 2)
        .await
        .expect("disable site before deletion");
    repository
        .delete_application(TENANT_A, site_id, Some(91))
        .await
        .expect("soft-delete site timestamps");
    repository
        .retrieve_application(TENANT_A, None, site_id)
        .await
        .expect_err("soft-deleted site must not be retrievable");
}

fn test_media_resource(node_id: &str, width: i32, height: i32) -> MediaResource {
    MediaResource {
        id: Some(node_id.to_string()),
        kind: "image".to_string(),
        source: "drive".to_string(),
        uri: Some(format!("drive://spaces/store-assets/nodes/{node_id}")),
        file_name: Some("application-icon.png".to_string()),
        mime_type: Some("image/png".to_string()),
        size_bytes: Some("4096".to_string()),
        width: Some(width),
        height: Some(height),
        alt_text: Some("Application icon".to_string()),
        metadata: Some(serde_json::json!({
            "drive": { "spaceId": "store-assets", "nodeId": node_id }
        })),
        ..MediaResource::default()
    }
}

async fn verify_runtime_assignment_contract(
    context: &TestContext,
    node_uuid: &str,
    assigned_site_uuid: &str,
) {
    let repository = &context.repository;
    let target = repository
        .resolve_runtime_assignment_target(TENANT_A, false, node_uuid)
        .await
        .expect("resolve tenant-owned runtime target");
    assert_eq!(target.node_uuid, node_uuid);
    assert_eq!(target.tenant_scope_hash, "a".repeat(64));
    assert_eq!(
        repository
            .resolve_runtime_assignment_target(0, true, node_uuid)
            .await
            .expect("authorized service resolves target tenant")
            .tenant_id,
        TENANT_A
    );
    assert_eq!(
        repository
            .resolve_runtime_assignment_target(TENANT_B, false, node_uuid)
            .await
            .expect_err("another tenant cannot resolve the target")
            .kind(),
        WebServiceErrorKind::NotFound
    );

    let production_one = runtime_assignment_write(&target, "production", 1, "production-one");
    let first = repository
        .publish_runtime_assignment(production_one.clone())
        .await
        .expect("publish first production assignment");
    let replay = repository
        .publish_runtime_assignment(production_one.clone())
        .await
        .expect("same generation and hash are idempotent");
    assert_eq!(replay.assignment_uuid, first.assignment_uuid);

    let generation_conflict = repository
        .publish_runtime_assignment(runtime_assignment_write(
            &target,
            "production",
            1,
            "generation-conflict",
        ))
        .await
        .expect_err("same generation with another hash must conflict");
    assert_eq!(generation_conflict.kind(), WebServiceErrorKind::Conflict);

    let staging_one = runtime_assignment_write(&target, "staging", 1, "staging-one");
    repository
        .publish_runtime_assignment(staging_one.clone())
        .await
        .expect("environment generations are isolated");

    let initial = repository
        .retrieve_current_runtime_assignment(TENANT_A, node_uuid, "production", None, None)
        .await
        .expect("retrieve current production assignment");
    assert!(!initial.unchanged);
    assert_eq!(initial.assignment.generation, "1");
    assert!(initial.runtime_set.is_some());
    let unchanged = repository
        .retrieve_current_runtime_assignment(
            TENANT_A,
            node_uuid,
            "production",
            Some(&initial.assignment.generation),
            Some(&initial.assignment.snapshot_sha256),
        )
        .await
        .expect("conditionally retrieve current assignment");
    assert!(unchanged.unchanged);
    assert!(unchanged.runtime_set.is_none());
    let changed = repository
        .retrieve_current_runtime_assignment(
            TENANT_A,
            node_uuid,
            "production",
            Some(&initial.assignment.generation),
            Some(&"f".repeat(64)),
        )
        .await
        .expect("a mismatched condition returns the assignment body");
    assert!(!changed.unchanged);
    assert!(changed.runtime_set.is_some());
    assert_eq!(
        repository
            .retrieve_current_runtime_assignment(TENANT_B, node_uuid, "production", None, None,)
            .await
            .expect_err("current assignment is tenant scoped")
            .kind(),
        WebServiceErrorKind::NotFound
    );

    let production_two = runtime_assignment_write(&target, "production", 2, "production-two");
    let second = repository
        .publish_runtime_assignment(production_two.clone())
        .await
        .expect("publish next production generation");
    assert_eq!(second.generation, "2");
    assert_eq!(
        repository
            .retrieve_latest_runtime_observation(TENANT_A, false, &production_two.snapshot_uuid,)
            .await
            .expect_err("an assignment without observations is not an observation resource")
            .kind(),
        WebServiceErrorKind::NotFound
    );
    assert_eq!(
        repository
            .publish_runtime_assignment(production_one)
            .await
            .expect_err("a lower generation must remain stale")
            .kind(),
        WebServiceErrorKind::Conflict
    );

    let active_first = runtime_observation_write(
        &production_two,
        RuntimeObservationState::Active,
        Some("1.0.0"),
        None,
        None,
    );
    assert_eq!(
        repository
            .create_runtime_observation(active_first)
            .await
            .expect_err("observations cannot start at ACTIVE")
            .kind(),
        WebServiceErrorKind::Conflict
    );

    let received_write = runtime_observation_write(
        &production_two,
        RuntimeObservationState::Received,
        Some("1.0.0"),
        None,
        None,
    );
    let received = repository
        .create_runtime_observation(received_write.clone())
        .await
        .expect("record RECEIVED");
    let received_replay = repository
        .create_runtime_observation(received_write.clone())
        .await
        .expect("identical observation is idempotent");
    assert_eq!(received_replay.observation_uuid, received.observation_uuid);
    let latest_received = repository
        .retrieve_latest_runtime_observation(TENANT_A, false, &production_two.snapshot_uuid)
        .await
        .expect("tenant retrieves its latest observation");
    assert_eq!(latest_received, received);
    assert_eq!(latest_received.environment, "production");
    assert_eq!(latest_received.assignment_uuid, second.assignment_uuid);
    assert_eq!(
        repository
            .retrieve_latest_runtime_observation(TENANT_B, false, &production_two.snapshot_uuid,)
            .await
            .expect_err("another tenant cannot retrieve the observation")
            .kind(),
        WebServiceErrorKind::NotFound
    );
    assert_eq!(
        repository
            .retrieve_latest_runtime_observation(0, true, &production_two.snapshot_uuid)
            .await
            .expect("authorized control plane retrieves a tenant observation"),
        received
    );
    let mut changed_received = received_write;
    changed_received.node_version = Some("1.0.1".to_owned());
    assert_eq!(
        repository
            .create_runtime_observation(changed_received)
            .await
            .expect_err("same state cannot be replayed with another payload")
            .kind(),
        WebServiceErrorKind::Conflict
    );
    assert_eq!(
        repository
            .create_runtime_observation(runtime_observation_write(
                &production_two,
                RuntimeObservationState::Staged,
                Some("1.0.0"),
                None,
                None,
            ))
            .await
            .expect_err("normal observation phases cannot be skipped")
            .kind(),
        WebServiceErrorKind::Conflict
    );

    for state in [
        RuntimeObservationState::Validated,
        RuntimeObservationState::Staged,
        RuntimeObservationState::Active,
    ] {
        repository
            .create_runtime_observation(runtime_observation_write(
                &production_two,
                state,
                Some("1.0.0"),
                None,
                None,
            ))
            .await
            .expect("advance observation state");
    }
    assert_eq!(
        repository
            .retrieve_current_runtime_assignment(
                TENANT_A,
                node_uuid,
                "production",
                Some(&production_two.generation.to_string()),
                Some(&production_two.snapshot_sha256),
            )
            .await
            .expect("retrieve activation checkpoint")
            .latest_observation_state,
        Some(RuntimeObservationState::Active)
    );
    assert_eq!(
        repository
            .retrieve_latest_runtime_observation(TENANT_A, false, &production_two.snapshot_uuid,)
            .await
            .expect("retrieve the active observation")
            .state,
        RuntimeObservationState::Active
    );
    assert_eq!(
        repository
            .create_runtime_observation(runtime_observation_write(
                &production_two,
                RuntimeObservationState::Rejected,
                Some("1.0.0"),
                Some("ACTIVATION_FAILED"),
                Some("must not replace ACTIVE"),
            ))
            .await
            .expect_err("terminal observations are immutable")
            .kind(),
        WebServiceErrorKind::Conflict
    );

    repository
        .create_runtime_observation(runtime_observation_write(
            &staging_one,
            RuntimeObservationState::Received,
            Some("1.0.0"),
            None,
            None,
        ))
        .await
        .expect("record staging RECEIVED");
    repository
        .create_runtime_observation(runtime_observation_write(
            &staging_one,
            RuntimeObservationState::Rejected,
            Some("1.0.0"),
            Some("VALIDATION_FAILED"),
            Some("synthetic parity rejection"),
        ))
        .await
        .expect("REJECTED may terminate any non-terminal phase");
    let mut generation_mismatch = runtime_observation_write(
        &staging_one,
        RuntimeObservationState::Rejected,
        Some("1.0.0"),
        Some("VALIDATION_FAILED"),
        Some("synthetic parity rejection"),
    );
    generation_mismatch.generation += 1;
    assert_eq!(
        repository
            .create_runtime_observation(generation_mismatch)
            .await
            .expect_err("observation generation must match assignment")
            .kind(),
        WebServiceErrorKind::Conflict
    );
    sqlx::query(
        "UPDATE web_runtime_assignment a
         SET runtime_set = jsonb_set(
             a.runtime_set,
             '{descriptors}',
             jsonb_build_array(jsonb_build_object('siteUuid', CAST($3 AS TEXT))),
             FALSE
         )
         FROM web_server s
         WHERE a.tenant_id = $1 AND a.server_id = s.id AND s.uuid = $2",
    )
    .bind(TENANT_A)
    .bind(node_uuid)
    .bind(assigned_site_uuid)
    .execute(&context.pool)
    .await
    .expect("scope current runtime assignments to the parity Site");
}

fn runtime_assignment_write(
    target: &RuntimeAssignmentTarget,
    environment: &str,
    generation: u64,
    identity: &str,
) -> RuntimeAssignmentWrite {
    let snapshot_uuid = format!("snapshot-{generation}-{identity}");
    let mut value = serde_json::json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": snapshot_uuid,
        "nodeUuid": target.node_uuid,
        "environment": environment,
        "generation": generation,
        "generatedAt": "2026-07-22T00:00:00Z",
        "compilerVersion": "repository-parity/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 8,
        "descriptors": []
    });
    let unsigned: WebsiteRuntimeSetSnapshot =
        serde_json::from_value(value.clone()).expect("parse unsigned runtime-set fixture");
    let snapshot_sha256 =
        website_runtime_set_snapshot_sha256(&unsigned).expect("hash runtime-set fixture");
    value["snapshotSha256"] = serde_json::Value::String(snapshot_sha256.clone());
    RuntimeAssignmentWrite {
        tenant_id: target.tenant_id,
        server_id: target.server_id,
        node_uuid: target.node_uuid.clone(),
        environment: environment.to_owned(),
        generation,
        snapshot_uuid,
        snapshot_sha256,
        runtime_set_json: serde_json::to_string(&value).expect("serialize runtime-set fixture"),
        runtime_set_bytes: serde_json::to_vec(&value)
            .expect("measure runtime-set fixture")
            .len(),
        assigned_by_subject: "repository-parity".to_owned(),
    }
}

fn runtime_observation_write(
    assignment: &RuntimeAssignmentWrite,
    state: RuntimeObservationState,
    node_version: Option<&str>,
    reason_code: Option<&str>,
    detail: Option<&str>,
) -> RuntimeObservationWrite {
    RuntimeObservationWrite {
        tenant_id: assignment.tenant_id,
        node_uuid: assignment.node_uuid.clone(),
        snapshot_uuid: assignment.snapshot_uuid.clone(),
        generation: assignment.generation,
        snapshot_sha256: assignment.snapshot_sha256.clone(),
        state,
        node_version: node_version.map(str::to_owned),
        reason_code: reason_code.map(str::to_owned),
        detail: detail.map(str::to_owned),
    }
}

async fn verify_node_sync_database_bounds(
    context: &TestContext,
    server_id: &str,
    nginx_config_id: &str,
    certificate_id: &str,
) {
    let original_config: String = sqlx::query_scalar(
        "SELECT config_content FROM web_nginx_config WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(nginx_config_id)
    .fetch_one(&context.pool)
    .await
    .expect("read original node sync config");
    sqlx::query(
        "UPDATE web_nginx_config SET config_content = $1 WHERE tenant_id = $2 AND uuid = $3",
    )
    .bind("x".repeat(1024 * 1024 + 1))
    .bind(TENANT_A)
    .bind(nginx_config_id)
    .execute(&context.pool)
    .await
    .expect("install oversized node sync config");
    let oversized_config = context
        .repository
        .build_agent_sync_manifest(server_id, TENANT_A, None)
        .await
        .expect_err("oversized node sync config must fail closed");
    assert!(oversized_config
        .to_string()
        .contains("active nginx configuration exceeds"));
    sqlx::query(
        "UPDATE web_nginx_config SET config_content = $1 WHERE tenant_id = $2 AND uuid = $3",
    )
    .bind(original_config)
    .bind(TENANT_A)
    .bind(nginx_config_id)
    .execute(&context.pool)
    .await
    .expect("restore node sync config");

    let original_metadata: String = sqlx::query_scalar(
        "SELECT CAST(metadata AS TEXT) FROM web_certificate WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(certificate_id)
    .fetch_one(&context.pool)
    .await
    .expect("read original node sync certificate metadata");
    let unrelated_metadata = serde_json::json!({
        "legacyCertificateMaterial": "must-not-be-consumed",
        "padding": "x".repeat(2 * 1024 * 1024),
    })
    .to_string();
    let metadata_update =
        "UPDATE web_certificate SET metadata = CAST($1 AS JSONB) WHERE tenant_id = $2 AND uuid = $3";
    sqlx::query(metadata_update)
        .bind(unrelated_metadata)
        .bind(TENANT_A)
        .bind(certificate_id)
        .execute(&context.pool)
        .await
        .expect("install unrelated certificate metadata");
    let sync = context
        .repository
        .build_agent_sync_manifest(server_id, TENANT_A, None)
        .await
        .expect("certificate aggregate metadata must not feed node secret distribution");
    assert_eq!(sync.certificates.len(), 1);
    assert!(sync.certificates[0]
        .privkey_pem
        .contains("BEGIN PRIVATE KEY"));
    sqlx::query(metadata_update)
        .bind(original_metadata)
        .bind(TENANT_A)
        .bind(certificate_id)
        .execute(&context.pool)
        .await
        .expect("restore node sync certificate metadata");
}

async fn verify_deployment_idempotency(
    context: &TestContext,
    first_site_id: &str,
    second_site_id: &str,
) {
    let repository = context.repository.as_ref();
    let request = CreateDeploymentRequest {
        deploy_type: 1,
        source_version_id: None,
        environment: Some("production".to_owned()),
        version_tag: Some("v1.2.3".to_owned()),
        commit_hash: Some("0123456789abcdef".to_owned()),
        source_ref: Some("main".to_owned()),
        artifact_drive_uri: Some("drive://spaces/space-1/nodes/node-1".to_owned()),
        artifact_size: Some(4096),
        artifact_hash: Some("a".repeat(64)),
        idempotency_key: Some("deploy-idempotency-1".to_owned()),
    };
    let first = repository
        .create_deployment(TENANT_A, first_site_id, Some(91), &request)
        .await
        .expect("create idempotent deployment");
    let repeated = repository
        .create_deployment(TENANT_A, first_site_id, Some(91), &request)
        .await
        .expect("repeat identical deployment");
    assert_eq!(repeated.id, first.id);
    assert_eq!(first.environment, "production");
    assert_eq!(first.version_tag.as_deref(), Some("v1.2.3"));
    assert_eq!(
        first.artifact_drive_uri.as_deref(),
        Some("drive://spaces/space-1/nodes/node-1")
    );
    assert_eq!(first.artifact_size, Some(4096));
    assert!(first
        .artifact_hash
        .as_deref()
        .is_some_and(|hash| hash == "a".repeat(64)));
    let stored = sqlx::query(
        "SELECT idempotency_key, organization_id FROM web_deployment
         WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(TENANT_A)
    .bind(&first.id)
    .fetch_one(&context.pool)
    .await
    .expect("load persisted deployment idempotency identity");
    let stored_key: String = stored.try_get("idempotency_key").expect("idempotency hash");
    assert_eq!(stored_key.len(), 64);
    assert_ne!(stored_key, "deploy-idempotency-1");
    assert_eq!(
        stored
            .try_get::<i64, _>("organization_id")
            .expect("deployment organization"),
        31
    );

    let conflicting_input = repository
        .create_deployment(
            TENANT_A,
            first_site_id,
            Some(91),
            &CreateDeploymentRequest {
                deploy_type: 2,
                ..request.clone()
            },
        )
        .await
        .expect_err("same idempotency key with different input must conflict");
    assert_eq!(conflicting_input.kind(), WebServiceErrorKind::Conflict);
    let second_site = repository
        .create_deployment(TENANT_A, second_site_id, Some(91), &request)
        .await
        .expect("the same client key is independent on another resource path");
    assert_ne!(second_site.id, first.id);
    let second_actor = repository
        .create_deployment(TENANT_A, first_site_id, Some(92), &request)
        .await
        .expect("the same client key is independent for another principal");
    assert_ne!(second_actor.id, first.id);

    let concurrent_request = CreateDeploymentRequest {
        idempotency_key: Some("deploy-idempotency-race".to_owned()),
        ..request
    };
    let (left, right) = tokio::join!(
        repository.create_deployment(TENANT_A, first_site_id, Some(91), &concurrent_request),
        repository.create_deployment(TENANT_A, first_site_id, Some(91), &concurrent_request),
    );
    assert_eq!(
        left.expect("left concurrent idempotency result").id,
        right.expect("right concurrent idempotency result").id
    );
}

async fn verify_source_version_contract(
    context: &TestContext,
    site_id: &str,
    other_site_id: &str,
    other_tenant_site_id: &str,
) {
    let repository = context.repository.as_ref();
    let first_request = test_source_version_request(0);
    let first = repository
        .create_source_version(TENANT_A, site_id, Some(91), 5, &first_request)
        .await
        .expect("create first source version");
    assert_eq!(first.status, 1);
    assert!(first.retained);
    assert_eq!(
        first.config_snapshot.app_config_path,
        first_request.config_snapshot.app_config_path
    );
    assert_eq!(
        first.config_snapshot.deployment_config_path,
        first_request.config_snapshot.deployment_config_path
    );
    assert!(first.config_snapshot.app_config_detected);
    assert!(first.config_snapshot.deployment_config_detected);

    repository
        .retrieve_source_version(TENANT_A, other_site_id, &first.id)
        .await
        .expect_err("another site must not retrieve a source version");
    repository
        .retrieve_source_version(TENANT_B, other_tenant_site_id, &first.id)
        .await
        .expect_err("another tenant must not retrieve a source version");
    repository
        .create_source_version(TENANT_B, site_id, Some(91), 5, &first_request)
        .await
        .expect_err("another tenant must not create a source version for the site");

    repository
        .create_source_version(TENANT_A, other_site_id, Some(91), 5, &first_request)
        .await
        .expect("the same version tag is valid on another site");
    repository
        .create_source_version(TENANT_B, other_tenant_site_id, Some(91), 5, &first_request)
        .await
        .expect("the same version tag is valid in another tenant");
    let duplicate = repository
        .create_source_version(TENANT_A, site_id, Some(91), 5, &first_request)
        .await
        .expect_err("a version tag must be unique within one application");
    assert_eq!(duplicate.kind(), WebServiceErrorKind::Conflict);

    let mut created = vec![first];
    for index in 1..7 {
        created.push(
            repository
                .create_source_version(
                    TENANT_A,
                    site_id,
                    Some(91),
                    5,
                    &test_source_version_request(index),
                )
                .await
                .expect("create retained source version"),
        );
    }
    let page = repository
        .list_source_versions(TENANT_A, site_id, 1, i32::MAX, None)
        .await
        .expect_err("invalid page_size must be rejected");
    assert!(matches!(
        page,
        sdkwork_webserver_contract::WebServiceError::Validation(_)
    ));
    let page = repository
        .list_source_versions(TENANT_A, site_id, 1, 200, None)
        .await
        .expect("list retained and pruned source versions");
    assert_eq!(page.total, 7);
    assert_eq!(page.page_size, 200);
    assert_eq!(page.items.iter().filter(|item| item.retained).count(), 5);
    assert!(page.items[..5]
        .iter()
        .all(|item| item.status == 1 && item.retained));
    assert!(page.items[5..]
        .iter()
        .all(|item| item.status == 3 && !item.retained));

    let selected = created.last().expect("latest source version");
    let deployment = repository
        .create_deployment(
            TENANT_A,
            site_id,
            Some(91),
            &CreateDeploymentRequest {
                deploy_type: 1,
                source_version_id: Some(selected.id.clone()),
                environment: Some("production".to_owned()),
                version_tag: Some("release-v6".to_owned()),
                commit_hash: Some("caller-must-not-override".to_owned()),
                source_ref: Some("caller/must-not-override".to_owned()),
                artifact_drive_uri: Some("drive://spaces/untrusted/nodes/untrusted".to_owned()),
                artifact_size: Some(1),
                artifact_hash: Some("f".repeat(64)),
                idempotency_key: None,
            },
        )
        .await
        .expect("create deployment from retained source version");
    assert_eq!(
        deployment.source_version_id.as_deref(),
        Some(selected.id.as_str())
    );
    assert_eq!(deployment.version_tag.as_deref(), Some("release-v6"));
    assert_eq!(deployment.commit_hash, selected.commit_hash);
    assert_eq!(deployment.source_ref, selected.source_ref);
    assert_eq!(
        deployment.artifact_drive_uri.as_deref(),
        Some(selected.artifact_drive_uri.as_str())
    );
    assert_eq!(deployment.artifact_size, Some(selected.artifact_size));
    assert_eq!(
        deployment.artifact_hash.as_deref(),
        Some(selected.artifact_hash.as_str())
    );

    for (tenant_id, target_site_id) in [(TENANT_A, other_site_id), (TENANT_B, other_tenant_site_id)]
    {
        let error = repository
            .create_deployment(
                tenant_id,
                target_site_id,
                Some(91),
                &CreateDeploymentRequest {
                    source_version_id: Some(selected.id.clone()),
                    ..CreateDeploymentRequest::default()
                },
            )
            .await
            .expect_err("a deployment must not use another application source version");
        assert_eq!(error.kind(), WebServiceErrorKind::NotFound);
    }

    sqlx::query("UPDATE web_deployment SET status = 2 WHERE tenant_id = $1 AND uuid = $2")
        .bind(TENANT_A)
        .bind(&deployment.id)
        .execute(&context.pool)
        .await
        .expect("mark source-version deployment successful");
    for index in 7..12 {
        repository
            .create_source_version(
                TENANT_A,
                site_id,
                Some(91),
                5,
                &test_source_version_request(index),
            )
            .await
            .expect("advance source-version retention window");
    }
    let pruned = repository
        .retrieve_source_version(TENANT_A, site_id, &selected.id)
        .await
        .expect("retrieve pruned source version");
    assert_eq!(pruned.status, 3);
    assert!(!pruned.retained);
    let rollback_error = repository
        .rollback_deployment(TENANT_A, site_id, &deployment.id, Some(91), None)
        .await
        .expect_err("a successful deployment cannot restore a pruned source version");
    assert_eq!(rollback_error.kind(), WebServiceErrorKind::Conflict);
}

fn test_source_version_request(index: usize) -> CreateSourceVersionRequest {
    CreateSourceVersionRequest {
        version_tag: format!("source-v{index}"),
        source_type: "ARCHIVE".to_owned(),
        source_ref: Some(format!("release/source-v{index}")),
        commit_hash: Some(format!("{index:040x}")),
        artifact_drive_uri: format!("drive://spaces/source-versions/nodes/node-{index}"),
        artifact_size: 1024 + index as i64,
        artifact_hash: format!("{index:064x}"),
        config_snapshot: SourceVersionConfigSnapshot {
            app_config_path: "sdkwork.app.config.json".to_owned(),
            deployment_config_path: "etc/sdkwork.deployment.config.json".to_owned(),
            app_config_detected: true,
            deployment_config_detected: true,
        },
    }
}

async fn verify_rollback_atomicity(context: &TestContext, site_id: &str) {
    let source = context
        .repository
        .create_deployment(
            TENANT_A,
            site_id,
            Some(91),
            &CreateDeploymentRequest {
                deploy_type: 1,
                source_version_id: None,
                environment: Some("production".to_owned()),
                version_tag: Some("rollback-source".to_owned()),
                commit_hash: None,
                source_ref: Some("release/rollback-source".to_owned()),
                artifact_drive_uri: Some(
                    "drive://spaces/space-rollback/nodes/node-rollback".to_owned(),
                ),
                artifact_size: Some(2048),
                artifact_hash: Some("b".repeat(64)),
                idempotency_key: None,
            },
        )
        .await
        .expect("create rollback source");
    let pending_rollback = context
        .repository
        .rollback_deployment(TENANT_A, site_id, &source.id, Some(91), None)
        .await
        .expect_err("pending deployment rollback must be rejected");
    assert_eq!(pending_rollback.kind(), WebServiceErrorKind::Conflict);

    sqlx::query("UPDATE web_deployment SET status = 2 WHERE tenant_id = $1 AND uuid = $2")
        .bind(TENANT_A)
        .bind(&source.id)
        .execute(&context.pool)
        .await
        .expect("mark rollback source successful");

    install_rollback_failure_trigger(&context.pool).await;
    context
        .repository
        .rollback_deployment(
            TENANT_A,
            site_id,
            &source.id,
            Some(91),
            Some("restore-failure"),
        )
        .await
        .expect_err("forced rollback-record failure must abort transaction");

    let status: i32 =
        sqlx::query_scalar("SELECT status FROM web_deployment WHERE tenant_id = $1 AND uuid = $2")
            .bind(TENANT_A)
            .bind(&source.id)
            .fetch_one(&context.pool)
            .await
            .expect("read rollback source status");
    assert_eq!(status, 2, "failed transaction must restore source status");
    let rollback_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_deployment WHERE tenant_id = $1 AND rollback_from IS NOT NULL",
    )
    .bind(TENANT_A)
    .fetch_one(&context.pool)
    .await
    .expect("count rollback records after forced failure");
    assert_eq!(rollback_records, 0);

    remove_rollback_failure_trigger(&context.pool).await;
    let rollback = context
        .repository
        .rollback_deployment(
            TENANT_A,
            site_id,
            &source.id,
            Some(91),
            Some("restore-source-v1"),
        )
        .await
        .expect("rollback succeeds after removing failure trigger");
    assert_eq!(rollback.application_id, site_id);
    assert_eq!(rollback.version_tag.as_deref(), Some("rollback-source"));
    assert_eq!(
        rollback.artifact_drive_uri.as_deref(),
        Some("drive://spaces/space-rollback/nodes/node-rollback")
    );
    assert_eq!(rollback.artifact_size, Some(2048));
    assert!(rollback
        .artifact_hash
        .as_deref()
        .is_some_and(|hash| hash == "b".repeat(64)));
    assert_eq!(
        rollback.rollback_from_deployment_id.as_deref(),
        Some(source.id.as_str())
    );

    let repeated = context
        .repository
        .rollback_deployment(
            TENANT_A,
            site_id,
            &source.id,
            Some(91),
            Some("restore-source-v1"),
        )
        .await
        .expect("repeating the same restore command is idempotent");
    assert_eq!(repeated.id, rollback.id);

    let second_restore = context
        .repository
        .rollback_deployment(
            TENANT_A,
            site_id,
            &source.id,
            Some(91),
            Some("restore-source-v1-again"),
        )
        .await
        .expect("a successful version can be restored more than once");
    assert_ne!(second_restore.id, rollback.id);
    assert_eq!(
        second_restore.rollback_from_deployment_id.as_deref(),
        Some(source.id.as_str())
    );

    let rollback_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_deployment WHERE tenant_id = $1 AND rollback_from IS NOT NULL",
    )
    .bind(TENANT_A)
    .fetch_one(&context.pool)
    .await
    .expect("count immutable restore records");
    assert_eq!(rollback_records, 2);
    let source_status: i32 =
        sqlx::query_scalar("SELECT status FROM web_deployment WHERE tenant_id = $1 AND uuid = $2")
            .bind(TENANT_A)
            .bind(&source.id)
            .fetch_one(&context.pool)
            .await
            .expect("read committed rollback source status");
    assert_eq!(source_status, 2);
}

async fn verify_nginx_activation_rollback(
    context: &TestContext,
    site_id: &str,
    active_config_id: &str,
) {
    let blocked = context
        .repository
        .create_nginx_config(
            TENANT_A,
            &CreateNginxConfigRequest {
                site_id: site_id.to_string(),
                config_name: "blocked-activation.conf".to_string(),
                config_type: 1,
                config_content: "server { listen 8443 ssl; }".to_string(),
            },
        )
        .await
        .expect("create nginx config for activation rollback");
    install_nginx_activation_ignore_trigger(&context.pool).await;

    let error = context
        .repository
        .web_nginx_config(Some(TENANT_A), &blocked.id)
        .await
        .expect_err("a skipped target activation must abort the transaction");
    assert_eq!(error.kind(), WebServiceErrorKind::NotFound);

    let active: String = sqlx::query_scalar(
        "SELECT config.uuid
         FROM web_nginx_config config
         INNER JOIN web_site site
           ON site.id = config.application_id AND site.tenant_id = config.tenant_id
         WHERE config.tenant_id = $1 AND site.uuid = $2 AND config.is_active = TRUE",
    )
    .bind(TENANT_A)
    .bind(site_id)
    .fetch_one(&context.pool)
    .await
    .expect("failed target activation preserves the previous active config");
    assert_eq!(active, active_config_id);

    remove_nginx_activation_ignore_trigger(&context.pool).await;
}

async fn install_nginx_activation_ignore_trigger(pool: &PgPool) {
    sqlx::query(
        "CREATE FUNCTION sdkwork_test_ignore_nginx_activation() RETURNS trigger AS $$
         BEGIN
           IF NEW.config_name = 'blocked-activation.conf' AND NEW.is_active = TRUE THEN
             RETURN NULL;
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL nginx activation trigger function");
    sqlx::query(
        "CREATE TRIGGER sdkwork_test_ignore_nginx_activation
         BEFORE UPDATE OF is_active ON web_nginx_config
         FOR EACH ROW EXECUTE FUNCTION sdkwork_test_ignore_nginx_activation()",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL nginx activation trigger");
}

async fn remove_nginx_activation_ignore_trigger(pool: &PgPool) {
    sqlx::query("DROP TRIGGER sdkwork_test_ignore_nginx_activation ON web_nginx_config")
        .execute(pool)
        .await
        .expect("remove PostgreSQL nginx activation trigger");
    sqlx::query("DROP FUNCTION sdkwork_test_ignore_nginx_activation()")
        .execute(pool)
        .await
        .expect("remove PostgreSQL nginx activation trigger function");
}

async fn install_rollback_failure_trigger(pool: &PgPool) {
    sqlx::query(
        "CREATE FUNCTION sdkwork_test_reject_rollback_insert() RETURNS trigger AS $$
         BEGIN
           IF NEW.rollback_from IS NOT NULL THEN
             RAISE EXCEPTION 'forced rollback insert failure';
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL rollback failure function");
    sqlx::query(
        "CREATE TRIGGER sdkwork_test_reject_rollback_insert
         BEFORE INSERT ON web_deployment
         FOR EACH ROW EXECUTE FUNCTION sdkwork_test_reject_rollback_insert()",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL rollback failure trigger");
}

async fn remove_rollback_failure_trigger(pool: &PgPool) {
    sqlx::query("DROP TRIGGER sdkwork_test_reject_rollback_insert ON web_deployment")
        .execute(pool)
        .await
        .expect("remove PostgreSQL rollback failure trigger");
    sqlx::query("DROP FUNCTION sdkwork_test_reject_rollback_insert()")
        .execute(pool)
        .await
        .expect("remove PostgreSQL rollback failure function");
}

async fn install_certificate_finalize_failure_trigger(pool: &PgPool) {
    sqlx::query(
        "CREATE FUNCTION sdkwork_test_reject_certificate_finalize() RETURNS trigger AS $$
         BEGIN
           IF OLD.status = 0 AND NEW.status = 1 THEN
             RAISE EXCEPTION 'forced certificate finalize failure';
           END IF;
           RETURN NEW;
         END;
         $$ LANGUAGE plpgsql",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL certificate finalize failure function");
    sqlx::query(
        "CREATE TRIGGER sdkwork_test_reject_certificate_finalize
         BEFORE UPDATE OF status ON web_certificate
         FOR EACH ROW EXECUTE FUNCTION sdkwork_test_reject_certificate_finalize()",
    )
    .execute(pool)
    .await
    .expect("install PostgreSQL certificate finalize failure trigger");
}

async fn remove_certificate_finalize_failure_trigger(pool: &PgPool) {
    sqlx::query("DROP TRIGGER sdkwork_test_reject_certificate_finalize ON web_certificate")
        .execute(pool)
        .await
        .expect("remove PostgreSQL certificate finalize failure trigger");
    sqlx::query("DROP FUNCTION sdkwork_test_reject_certificate_finalize()")
        .execute(pool)
        .await
        .expect("remove PostgreSQL certificate finalize failure function");
}
