pub const PREFIX: &str = "/backend/v3/api";

pub const APPLICATIONS: &str = "/backend/v3/api/applications";
pub const APPLICATION: &str = "/backend/v3/api/applications/{applicationId}";
pub const APPLICATION_ACTIVATE: &str = "/backend/v3/api/applications/{applicationId}/activate";
pub const APPLICATION_PAUSE: &str = "/backend/v3/api/applications/{applicationId}/pause";
pub const APPLICATION_DOMAINS: &str = "/backend/v3/api/applications/{applicationId}/domains";
pub const APPLICATION_DOMAIN: &str =
    "/backend/v3/api/applications/{applicationId}/domains/{domainId}";
pub const APPLICATION_DOMAIN_VERIFY: &str =
    "/backend/v3/api/applications/{applicationId}/domains/{domainId}/verify";
pub const APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDINGS: &str =
    "/backend/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings";
pub const APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDING: &str =
    "/backend/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings/{bindingId}";
pub const ROOT_DOMAINS: &str = "/backend/v3/api/root_domains";
pub const ROOT_DOMAIN: &str = "/backend/v3/api/root_domains/{rootDomainId}";
pub const ROOT_DOMAIN_SUBDOMAINS: &str = "/backend/v3/api/root_domains/{rootDomainId}/subdomains";
pub const DOMAINS: &str = "/backend/v3/api/domains";
pub const DOMAIN: &str = "/backend/v3/api/domains/{domainId}";
pub const DOMAIN_VERIFY: &str = "/backend/v3/api/domains/{domainId}/verify";
pub const DOMAIN_APPLICATION_BINDING: &str =
    "/backend/v3/api/domains/{domainId}/application_binding";
pub const APPLICATION_SOURCE_VERSIONS: &str =
    "/backend/v3/api/applications/{applicationId}/source_versions";
pub const APPLICATION_SOURCE_VERSION_IMPORT_GIT: &str =
    "/backend/v3/api/applications/{applicationId}/source_versions/git_import";
pub const APPLICATION_SOURCE_VERSION: &str =
    "/backend/v3/api/applications/{applicationId}/source_versions/{sourceVersionId}";
pub const APPLICATION_DEPLOYMENTS: &str =
    "/backend/v3/api/applications/{applicationId}/deployments";
pub const APPLICATION_DEPLOYMENT_ROLLBACK: &str =
    "/backend/v3/api/applications/{applicationId}/deployments/{deploymentId}/rollback";
pub const CERTIFICATES: &str = "/backend/v3/api/certificates";
pub const CERTIFICATES_ISSUE: &str = "/backend/v3/api/certificates/issue";
pub const CERTIFICATE_OPERATION: &str = "/backend/v3/api/certificates/operations/{operationId}";
pub const CERTIFICATE: &str = "/backend/v3/api/certificates/{certificateId}";
pub const CERTIFICATE_RENEW: &str = "/backend/v3/api/certificates/{certificateId}/renew";
pub const CERTIFICATE_REVOKE: &str = "/backend/v3/api/certificates/{certificateId}/revoke";
pub const CERTIFICATE_DISTRIBUTION: &str = "/backend/v3/api/certificate_distribution";

pub const NGINX_CONFIGS: &str = "/backend/v3/api/nginx/configs";
pub const NGINX_CONFIG: &str = "/backend/v3/api/nginx/etc/{configId}";
pub const NGINX_CONFIG_VALIDATE: &str = "/backend/v3/api/nginx/etc/{configId}/validate";
pub const NGINX_CONFIG_DEPLOY: &str = "/backend/v3/api/nginx/etc/{configId}/deploy";
pub const NGINX_RELOAD: &str = "/backend/v3/api/nginx/reload";
pub const NGINX_STATUS: &str = "/backend/v3/api/nginx/status";
pub const SERVERS: &str = "/backend/v3/api/servers";
pub const SERVER_FILES_NODES: &str = "/backend/v3/api/server-files/nodes";
pub const SERVER_FILES_NODE_BROWSE: &str = "/backend/v3/api/server-files/nodes/{nodeId}/browse";
pub const SERVER_FILES_NODE_READ: &str = "/backend/v3/api/server-files/nodes/{nodeId}/read";
pub const SERVER_FILES_NODE_OPERATIONS: &str = "/backend/v3/api/server-files/nodes/{nodeId}/operations";
pub const AUDIT_LOGS: &str = "/backend/v3/api/audit_logs";
pub const AGENT_HEARTBEAT: &str = "/backend/v3/api/agent/heartbeat";
pub const AGENT_SYNC: &str = "/backend/v3/api/agent/sync";
