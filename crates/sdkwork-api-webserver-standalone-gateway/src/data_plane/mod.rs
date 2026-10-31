mod accept;
mod acme_challenge;
mod active_health;
mod autoindex;
pub(crate) mod cache;
mod cache_policy;
pub(crate) mod certificates_metrics;
mod cgi_cache;
mod cgi_upstream;
mod charset_policy;
mod connection_limit;
pub(crate) mod credential_entry_injection;
mod delivery_policy;
mod dns;
mod error;
mod fixed_histogram;
mod forwarded_scheme;
mod grpc_upstream;
mod gzip_predicate;
mod handler;
mod handshake_timeout;
mod http1_wire;
mod http2_wire;
mod io_timeout;
mod keep_alive_timeout;
mod limit_conn;
mod limit_req;
mod mail_frontend;
mod map_variables;
mod metrics;
mod operations;
mod protocol_pool;
mod provider_resource;
mod proxy;
pub(crate) mod proxy_body;
mod proxy_protocol;
mod proxy_response_policy;
mod real_ip;
mod request_admission;
mod request_body_timeout;
mod request_gate;
mod request_uri;
mod request_variables;
mod resource_pressure;
mod rewrite_phase;
mod runtime;
mod server;
mod smooth_weighted;
pub(crate) mod static_file_response;
pub(crate) mod static_files;
pub(crate) mod static_path;
mod stream_proxy;
mod stream_proxy_protocol;
mod stream_variables;
mod sub_filter;
mod subrequest;
mod tls;
mod tls_material;
mod tls_resolver;
mod tls_runtime;
mod tls_served_report;
mod tunnel;
mod udp_stream_proxy;
mod upstream_admission;
mod upstream_client;
mod upstream_tls;
mod watch;
mod website_delivery;
pub(crate) mod wechat_verify;

pub use error::DataPlaneError;
pub use operations::{probe_data_plane_operations_from_env, DataPlaneOperationsConfig};
pub use runtime::DataPlaneReloadReport;
pub use server::{
    run_data_plane_until, run_data_plane_until_with_cluster_overlay,
    run_data_plane_with_operations_until,
};
pub use server::{
    run_website_data_plane_until, run_website_data_plane_with_operations_until,
    run_website_data_plane_with_tls_operations_until,
};
pub use tls_runtime::{FileTlsRuntimeConfig, FileTlsRuntimeController, FileTlsRuntimeError};
pub use watch::{
    run_data_plane_from_config_until, run_data_plane_from_config_with_operations_until,
};

use std::sync::Arc;

use self::runtime::DataPlaneRuntime;

#[derive(Clone)]
struct ListenerState {
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<sdkwork_webserver_delivery_runtime::WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<sdkwork_webserver_delivery_runtime::AppConfigResourceExecutor>>,
    deploy_fallback: Option<Arc<crate::deploy_fallback::DeployFallbackResolver>>,
    usage_meter: Option<Arc<crate::usage_metering::UsageMeteringAggregator>>,
    listener_id: String,
    is_tls: bool,
}
