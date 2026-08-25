mod acme_challenge;
mod active_health;
pub(crate) mod cache;
mod connection_limit;
mod dns;
mod error;
mod fixed_histogram;
mod forwarded_scheme;
mod gzip_predicate;
mod handler;
mod http1_wire;
mod http2_wire;
mod io_timeout;
mod keep_alive_timeout;
mod limit_conn;
mod limit_req;
mod metrics;
mod operations;
mod provider_resource;
mod proxy;
mod proxy_body;
mod proxy_protocol;
mod real_ip;
mod request_admission;
mod request_body_timeout;
mod request_gate;
mod request_uri;
mod resource_pressure;
mod runtime;
mod server;
mod smooth_weighted;
pub(crate) mod static_file_response;
mod static_files;
pub(crate) mod static_path;
mod stream_proxy;
mod sub_filter;
mod tls;
mod tls_material;
mod tls_runtime;
mod tunnel;
mod upstream_admission;
mod udp_stream_proxy;
mod upstream_client;
mod upstream_tls;
mod watch;
mod website_delivery;

pub use error::DataPlaneError;
pub use operations::{probe_data_plane_operations_from_env, DataPlaneOperationsConfig};
pub use runtime::DataPlaneReloadReport;
pub use server::{run_data_plane_until, run_data_plane_with_operations_until};
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
