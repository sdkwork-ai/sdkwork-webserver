pub const PREFIX: &str = "/app/v3/api";

pub const APPLICATIONS: &str = "/app/v3/api/applications";
pub const APPLICATION: &str = "/app/v3/api/applications/{applicationId}";
pub const APPLICATION_ACTIVATE: &str = "/app/v3/api/applications/{applicationId}/activate";
pub const APPLICATION_PAUSE: &str = "/app/v3/api/applications/{applicationId}/pause";
pub const APPLICATION_DOMAINS: &str = "/app/v3/api/applications/{applicationId}/domains";
pub const APPLICATION_DOMAIN: &str = "/app/v3/api/applications/{applicationId}/domains/{domainId}";
pub const APPLICATION_DOMAIN_VERIFY: &str =
    "/app/v3/api/applications/{applicationId}/domains/{domainId}/verify";
pub const APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDINGS: &str =
    "/app/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings";
pub const APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDING: &str =
    "/app/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings/{bindingId}";
pub const APPLICATION_SOURCE_VERSIONS: &str =
    "/app/v3/api/applications/{applicationId}/source_versions";
pub const APPLICATION_SOURCE_VERSION: &str =
    "/app/v3/api/applications/{applicationId}/source_versions/{sourceVersionId}";
pub const APPLICATION_SOURCE_VERSION_GIT_IMPORT: &str =
    "/app/v3/api/applications/{applicationId}/source_versions/git_import";
pub const APPLICATION_DEPLOYMENTS: &str = "/app/v3/api/applications/{applicationId}/deployments";
pub const APPLICATION_DEPLOYMENT: &str =
    "/app/v3/api/applications/{applicationId}/deployments/{deploymentId}";
pub const APPLICATION_DEPLOYMENT_ROLLBACK: &str =
    "/app/v3/api/applications/{applicationId}/deployments/{deploymentId}/rollback";
pub const APPLICATION_ENV_VARIABLES: &str =
    "/app/v3/api/applications/{applicationId}/env_variables";
pub const APPLICATION_ENV_VARIABLE: &str =
    "/app/v3/api/applications/{applicationId}/env_variables/{variableId}";
pub const DOMAINS: &str = "/app/v3/api/domains";
pub const APPLICATION_HEALTH_CHECKS: &str =
    "/app/v3/api/applications/{applicationId}/health_checks";
