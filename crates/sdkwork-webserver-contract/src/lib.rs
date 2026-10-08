//! Web Server service and HTTP port contracts.

pub mod app_ports;
pub mod cluster;
pub mod dto;
pub mod internal_ports;
pub mod metrics;
pub mod observability;
pub mod problem;
pub mod provider;
pub mod usage;

pub use app_ports::{
    ListApplicationsQuery, ListAuditLogsQuery, ListDomainDnsRecordsQuery, ListRootDomainsQuery,
    ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED, RootDomainCloudAccountFilter, WebAppApi,
    WebAppRequestContext, WebAppResourceScope, WebBackendApi, WebBackendRequestContext,
};
pub use cluster::*;
pub use dto::*;
pub use internal_ports::*;
pub use metrics::*;
pub use problem::{WebServiceError, WebServiceErrorKind, WebServiceResult};
pub use provider::*;
pub use sdkwork_webserver_core::{
    DEFAULT_PLATFORM_OPERATOR_TENANT_ID, PLATFORM_OPERATOR_TENANT_ID_ENV,
    web_dev_auth_bypass_enabled, web_environment_name, web_is_platform_operator_tenant,
    web_is_production_like_environment, web_platform_operator_tenant_id,
    web_use_dev_inline_auth_resolver,
};
pub use usage::*;
