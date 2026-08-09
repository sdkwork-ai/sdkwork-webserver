//! Shared response adapters for SDKWork Web Server route crates.

pub mod pagination;
pub mod problem;
pub mod response;

pub mod correlation {
    pub use sdkwork_webserver_http_host::{
        resolved_trace_id, with_problem_correlation, WebProblemCorrelation,
    };
}

pub mod machine_credential {
    pub use sdkwork_webserver_http_host::MachineCredentialResolverDecorator;
}

pub use correlation::{with_problem_correlation, WebProblemCorrelation};
pub use machine_credential::MachineCredentialResolverDecorator;
pub use pagination::validate_pagination_query;
pub use problem::{WebApiError, WebApiResult};
pub use response::{
    accepted_async, created_resource, no_content, ok_application_page, ok_audit_log_page,
    ok_certificate_distribution_page, ok_certificate_page, ok_deployment_page, ok_domain_page,
    ok_env_variable_page, ok_health_check_page, ok_listener_certificate_binding_page,
    ok_nginx_config_page, ok_platform_target_page, ok_resource, ok_root_domain_page, ok_server_page,
    ok_source_version_page,
};
pub use sdkwork_webserver_http_host::{
    web_auth_mode_from_env, web_framework_runtime_policy_from_env, ProductionFailClosedResolver,
    WebAuthMode, WebServerTenantIsolationPolicy,
};
