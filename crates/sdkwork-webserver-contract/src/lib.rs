//! Web Server service and HTTP port contracts.

pub mod app_ports;
pub mod dto;
pub mod internal_ports;
pub mod problem;
pub mod provider;

pub use app_ports::{
    ListApplicationsQuery, ListAuditLogsQuery, ListRootDomainsQuery, WebAppApi,
    WebAppRequestContext, WebAppResourceScope, WebBackendApi, WebBackendRequestContext,
};
pub use dto::*;
pub use internal_ports::*;
pub use problem::{WebServiceError, WebServiceErrorKind, WebServiceResult};
pub use provider::*;
pub use sdkwork_webserver_core::{
    web_dev_auth_bypass_enabled, web_environment_name, web_is_production_like_environment,
    web_use_dev_inline_auth_resolver,
};
