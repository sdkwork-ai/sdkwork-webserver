use std::{error::Error, io, path::PathBuf};

use sdkwork_api_webserver_standalone_gateway::{
    probe_data_plane_operations_from_env, run_website_data_plane_from_config_until,
    DataPlaneOperationsConfig, WEBSITE_RUNTIME_SET_FILE_ENV,
};
use sdkwork_webserver_core::{
    load_and_compile_webserver_config_revision, resolve_webserver_config_path,
};
use tokio::signal;

mod provider_event_relay;

type MainResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    if let Err(error) = run().await {
        tracing::error!(error = %error, "website delivery edge runtime failed");
        std::process::exit(1);
    }
}

async fn run() -> MainResult<()> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        None | Some("serve") => {
            let host_config = config_path(arguments.next())?;
            let runtime_set = runtime_set_path(arguments.next());
            reject_extra_arguments(arguments)?;
            let operations = DataPlaneOperationsConfig::from_env().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("data-plane operations config is invalid: {error}"),
                )
            })?;
            run_website_data_plane_from_config_until(
                host_config,
                runtime_set,
                operations,
                shutdown_signal(),
            )
            .await?;
        }
        Some("probe") => {
            let path = arguments.next().unwrap_or_else(|| "/readyz".to_owned());
            reject_extra_arguments(arguments)?;
            probe_data_plane_operations_from_env(&path)
                .await
                .map_err(|error| io::Error::other(format!("operations probe failed: {error}")))?;
        }
        Some("validate") => {
            let host_config = config_path(arguments.next())?;
            reject_extra_arguments(arguments)?;
            validate_host_config(host_config)?;
        }
        Some("relay-provider-events") => {
            reject_extra_arguments(arguments)?;
            provider_event_relay::run_from_env_until(shutdown_signal()).await?;
        }
        Some("help" | "--help" | "-h") => print_help(),
        Some(command) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown operation {command}; run with --help"),
            )
            .into())
        }
    }
    Ok(())
}

fn validate_host_config(path: PathBuf) -> MainResult<()> {
    let revision = load_and_compile_webserver_config_revision(&path).inspect_err(|error| {
        for diagnostic in error.diagnostics() {
            tracing::error!(
                config_path = %diagnostic.path,
                message = %diagnostic.message,
                "website host config diagnostic"
            );
        }
    })?;
    let compiled = revision.app();
    let route_count = compiled
        .config()
        .virtual_hosts
        .iter()
        .map(|virtual_host| virtual_host.routes.len())
        .sum::<usize>();
    println!(
        "validated appKey={} revision={} bytes={} listeners={} virtualHosts={} routes={} resources={}",
        compiled.config().app_key,
        revision.sha256(),
        revision.size_bytes(),
        compiled.config().listeners.len(),
        compiled.config().virtual_hosts.len(),
        route_count,
        compiled.config().resources.len(),
    );
    Ok(())
}

fn config_path(argument: Option<String>) -> MainResult<PathBuf> {
    resolve_webserver_config_path(argument)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message).into())
}

fn runtime_set_path(argument: Option<String>) -> Option<PathBuf> {
    argument
        .or_else(|| std::env::var(WEBSITE_RUNTIME_SET_FILE_ENV).ok())
        .map(PathBuf::from)
}

fn reject_extra_arguments(mut arguments: impl Iterator<Item = String>) -> MainResult<()> {
    if let Some(argument) = arguments.next() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unexpected argument {argument}"),
        )
        .into());
    }
    Ok(())
}

fn print_help() {
    println!(
        "sdkwork-webserver-website-delivery-edge-runtime\n\
         \n\
         Operations:\n\
           serve [host-config] [runtime-set]\n\
                                  Start immutable website/Wiki delivery listeners (default).\n\
                                  File source may use SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_FILE;\n\
                                  cloud source is selected by SDKWORK_WEBSERVER_RUNTIME_ASSIGNMENT_SOURCE.\n\
           probe [health-path]    Probe the loopback operations listener; defaults to /readyz.\n\
           validate [host-config] Validate and compile the website listener configuration.\n\
           relay-provider-events Relay bounded internal callback connections to the loopback receiver.\n\
         \n\
         Config resolution: explicit host-config argument, then\n\
         SDKWORK_WEBSERVER_SERVER_CONFIG_FILE, then the canonical OS config\n\
         directory (Linux /etc/sdkwork/webserver, macOS\n\
         /Library/Application Support/sdkwork/webserver, Windows\n\
         %ProgramData%\\sdkwork\\webserver) joined with sdkwork.webserver.config.json.\n"
    );
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = signal::ctrl_c().await {
            tracing::error!(error = %error, "failed to receive Ctrl+C signal");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to install SIGTERM handler");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
