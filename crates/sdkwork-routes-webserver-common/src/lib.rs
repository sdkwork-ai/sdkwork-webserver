//! Shared response adapters for SDKWork Web Server route crates.

pub mod pagination;
pub mod problem;
pub mod response;

pub mod correlation {
    pub use sdkwork_webserver_http_host::{
        WebProblemCorrelation, resolved_trace_id, with_problem_correlation,
    };
}

pub mod machine_credential {
    pub use sdkwork_webserver_http_host::MachineCredentialResolverDecorator;
}

pub use correlation::{WebProblemCorrelation, with_problem_correlation};
pub use machine_credential::MachineCredentialResolverDecorator;
pub use pagination::validate_pagination_query;
pub use problem::{WebApiError, WebApiResult};
pub use response::{
    BOUNDED_COLLECTION_MAXIMUM_PAGE_SIZE, accepted_async, created_resource, no_content,
    ok_application_page, ok_audit_log_page, ok_certificate_distribution_page, ok_certificate_page,
    ok_cluster_event_page, ok_cluster_heartbeat_page, ok_cluster_host_page,
    ok_cluster_instance_page, ok_cluster_page, ok_deployment_page, ok_dns_account_page,
    ok_domain_dns_record_page, ok_domain_page, ok_listener_certificate_binding_page,
    ok_nginx_config_page, ok_platform_target_page, ok_resource, ok_root_domain_page,
    ok_server_page, ok_source_version_page,
};
pub use sdkwork_webserver_http_host::{
    ProductionFailClosedResolver, WebAuthMode, WebServerTenantIsolationPolicy,
    web_auth_mode_from_env, web_framework_runtime_policy_from_env,
};
