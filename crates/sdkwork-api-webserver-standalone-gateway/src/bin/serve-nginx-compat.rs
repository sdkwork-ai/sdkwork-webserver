//! Dedicated nginx-compatibility regression binary.
//!
//! `sdkwork-api-webserver-standalone-gateway serve-nginx` is gated behind the
//! `management` feature (the full management assembly). The nginx behavioral
//! regression corpus under `tests/nginx/` only needs the data plane, so this
//! bin exposes the same `serve-nginx` path without the management feature set:
//!
//! ```sh
//! cargo build -p sdkwork-api-webserver-standalone-gateway \
//!   --no-default-features --bin serve-nginx-compat
//! serve-nginx-compat tests/nginx/full-surface/nginx.rendered.conf
//! ```

use std::io;
use std::path::PathBuf;

use sdkwork_api_webserver_standalone_gateway::run_data_plane_with_operations_until;
use sdkwork_webserver_core::config::{ConfigFormat, ConfigLoadOptions, WebServerConfigLoader};

type MainResult<T> = Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> MainResult<()> {
    let Some(path) = std::env::args().nth(1).map(PathBuf::from) else {
        return Err(io::Error::other("usage: serve-nginx-compat <nginx.conf>").into());
    };
    let loader = WebServerConfigLoader::new();
    let options = ConfigLoadOptions {
        format: Some(ConfigFormat::NginxConf),
        ..ConfigLoadOptions::default()
    };
    let loaded = loader.load(&path, &options)?;
    for (file, error) in &loaded.skipped {
        tracing::warn!(file = %file.display(), error = %error, "nginx site skipped (unsupported directives)");
    }
    tracing::info!(
        path = %path.display(),
        virtual_hosts = loaded.app.virtual_hosts.len(),
        upstreams = loaded.app.upstreams.len(),
        streams = loaded.app.streams.len(),
        skipped = loaded.skipped.len(),
        "nginx compatibility configuration materialized"
    );
    let compiled = loader.load_and_compile(&path, &options)?;
    run_data_plane_with_operations_until(compiled, None, shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %error, "failed to receive Ctrl+C signal");
    }
}
