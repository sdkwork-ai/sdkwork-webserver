use std::{error::Error, io, path::PathBuf};

use sdkwork_api_webserver_standalone_gateway::{
    build_router, configure_packaged_runtime_roots_from_env,
    run_data_plane_from_config_with_operations_until, run_data_plane_with_operations_until,
    run_database_migrate_only, validate_adaptive_app_shell_from_env, DataPlaneOperationsConfig,
};
use sdkwork_webserver_core::{
    resolve_webserver_config_path, validate_configured_module_imports, ConfigFormat,
    ConfigLoadOptions, WebServerConfigLoader,
};
use tokio::signal;

type MainResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    sdkwork_database_sqlx::enable_process_shared_database_pool();
    if let Err(error) = run().await {
        tracing::error!(error = %error, "sdkwork-api-webserver-standalone-gateway failed");
        std::process::exit(1);
    }
}

async fn run() -> MainResult<()> {
    // Installed deployments load the typed runtime configuration
    // (/etc/sdkwork/webserver/config.toml) and materialize it into
    // the process environment before any runtime component reads it.
    sdkwork_webserver_core::runtime_config::load_runtime_toml_config().map_err(|error| {
        io::Error::other(format!("runtime TOML configuration is invalid: {error}"))
    })?;
    validate_imported_module_webserver_configs()?;
    let raw_arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (format_override, arguments) = extract_format_override(&raw_arguments)?;
    let mut arguments = arguments.into_iter();
    let nginx_compat_command = {
        let mut peek = arguments.clone();
        matches!(
            peek.next().as_deref(),
            Some("serve-nginx") | Some("validate-nginx")
        )
    };
    // The nginx compatibility mode serves stock nginx configuration and does
    // not require the application's packaged dependency roots (IAM/Drive).
    if !nginx_compat_command {
        configure_packaged_runtime_roots_from_env().map_err(|error| {
            io::Error::other(format!("packaged runtime roots are invalid: {error}"))
        })?;
    }
    match arguments.next().as_deref() {
        None | Some("serve-management") => run_management_plane().await?,
        Some("db-migrate") => run_database_migrate_only()
            .await
            .map_err(|error| io::Error::other(format!("database migration failed: {error}")))?,
        Some("validate") => validate_config(config_path(arguments.next())?, format_override)?,
        Some("validate-module-imports") => validate_module_imports_command()?,
        Some("validate-app-shell") => {
            validate_adaptive_app_shell_from_env().map_err(|error| {
                io::Error::other(format!("Adaptive Web app shell validation failed: {error}"))
            })?;
            println!("validated standalone Adaptive Web app shell");
        }
        Some("data-plane") => {
            let operations = DataPlaneOperationsConfig::from_env().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("data-plane operations config is invalid: {error}"),
                )
            })?;
            run_data_plane_command(
                config_path(arguments.next())?,
                format_override,
                operations,
                shutdown_signal(),
            )
            .await?;
        }
        Some("serve-nginx") => {
            reject_format_for_nginx(format_override)?;
            run_nginx_compat_until(arguments.next(), shutdown_signal()).await?;
        }
        Some("validate-nginx") => {
            reject_format_for_nginx(format_override)?;
            validate_nginx_compat(arguments.next())?;
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

/// Extract `--format <json|toml|nginx>` (or `--format=…`) tokens from the
/// argument list, leaving the positional arguments in order.
fn extract_format_override(
    arguments: &[String],
) -> MainResult<(Option<ConfigFormat>, Vec<String>)> {
    let mut format = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        let token = &arguments[index];
        if token == "--format" {
            index += 1;
            let value = arguments.get(index).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--format requires a value: json|toml|nginx",
                )
            })?;
            format = Some(parse_config_format(value)?);
        } else if let Some(value) = token.strip_prefix("--format=") {
            format = Some(parse_config_format(value)?);
        } else {
            positional.push(token.clone());
        }
        index += 1;
    }
    Ok((format, positional))
}

fn parse_config_format(value: &str) -> MainResult<ConfigFormat> {
    match value {
        "json" => Ok(ConfigFormat::Json),
        "toml" => Ok(ConfigFormat::Toml),
        "nginx" => Ok(ConfigFormat::NginxConf),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown config format `{other}`; expected json|toml|nginx"),
        )
        .into()),
    }
}

fn reject_format_for_nginx(format: Option<ConfigFormat>) -> MainResult<()> {
    if let Some(format) = format {
        if format != ConfigFormat::NginxConf {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "serve-nginx / validate-nginx always load nginx configuration; remove --format",
            )
            .into());
        }
    }
    Ok(())
}

/// The unified config loader for all supported formats.
fn config_loader() -> WebServerConfigLoader {
    WebServerConfigLoader::new()
}

fn load_options(format: Option<ConfigFormat>) -> ConfigLoadOptions {
    ConfigLoadOptions {
        format,
        ..ConfigLoadOptions::default()
    }
}

/// nginx configuration compatibility mode: load a stock nginx config file
/// (`nginx.conf`) or a directory of `sites-enabled`-style `*.conf` files,
/// materialize it into the runtime model, and serve it with the data plane.
/// Files that declare directives the runtime cannot enforce are skipped with
/// a warning (progressive compatibility); at least one server must load.
async fn run_nginx_compat_until(
    configured: Option<String>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> MainResult<()> {
    let path = config_path(configured)?;
    let loader = config_loader();
    let options = load_options(Some(ConfigFormat::NginxConf));
    let loaded = loader
        .load(&path, &options)
        .map_err(|error| io::Error::other(format!("nginx materialization failed: {error}")))?;
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
    let compiled = loader
        .load_and_compile(&path, &options)
        .map_err(|error| io::Error::other(format!("nginx materialization failed: {error}")))?;
    run_data_plane_with_operations_until(compiled, None, shutdown).await?;
    Ok(())
}

/// Validate nginx compatibility materialization and print the effective
/// surface without starting listeners.
fn validate_nginx_compat(configured: Option<String>) -> MainResult<()> {
    let path = config_path(configured)?;
    let loader = config_loader();
    let options = load_options(Some(ConfigFormat::NginxConf));
    let loaded = loader
        .load(&path, &options)
        .map_err(|error| io::Error::other(format!("nginx materialization failed: {error}")))?;
    for (file, error) in &loaded.skipped {
        tracing::warn!(file = %file.display(), error = %error, "nginx site skipped");
    }
    println!(
        "validated nginx compatibility: appKey={} virtualHosts={} listeners={} resources={} upstreams={} streams={} skipped={}",
        loaded.app.app_key,
        loaded.app.virtual_hosts.len(),
        loaded.app.listeners.len(),
        loaded.app.resources.len(),
        loaded.app.upstreams.len(),
        loaded.app.streams.len(),
        loaded.skipped.len(),
    );
    Ok(())
}

/// `data-plane` with the unified config loader. JSON configurations keep
/// their revision-based watch reload; TOML and nginx configurations are
/// loaded once (their multi-file nature has no single-file revision).
async fn run_data_plane_command(
    path: PathBuf,
    format_override: Option<ConfigFormat>,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> MainResult<()> {
    let loader = config_loader();
    let options = load_options(format_override);
    let format = loader.format_of(&path, &options).map_err(|error| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("{error}"))
    })?;
    if format == ConfigFormat::Json {
        run_data_plane_from_config_with_operations_until(path, operations, shutdown).await?;
        return Ok(());
    }
    let compiled = loader
        .load_and_compile(&path, &options)
        .map_err(|error| io::Error::other(format!("config materialization failed: {error}")))?;
    run_data_plane_with_operations_until(compiled, operations, shutdown).await?;
    Ok(())
}

/// Load a configuration of any supported format and print the effective
/// surface without starting listeners.
fn validate_config(path: PathBuf, format_override: Option<ConfigFormat>) -> MainResult<()> {
    let loader = config_loader();
    let options = load_options(format_override);
    let revision = loader.load(&path, &options).inspect_err(|error| {
        for diagnostic in error.diagnostics() {
            tracing::error!(
                config_path = %diagnostic.path,
                message = %diagnostic.message,
                "Web Server config diagnostic"
            );
        }
    })?;
    let compiled = loader
        .load_and_compile(&path, &options)
        .map_err(|error| io::Error::other(format!("config compile failed: {error}")))?;
    let route_count = compiled
        .config()
        .virtual_hosts
        .iter()
        .map(|virtual_host| virtual_host.routes.len())
        .sum::<usize>();
    let revision_text = revision
        .revision
        .as_ref()
        .map(|value| value.sha256().to_owned())
        .unwrap_or_else(|| "-".to_owned());
    println!(
        "validated format={} appKey={} revision={} bytes={} listeners={} virtualHosts={} routes={} resources={} upstreams={} tlsPolicies={} skipped={}",
        revision.format.as_str(),
        compiled.config().app_key,
        revision_text,
        revision.revision.as_ref().map(|value| value.size_bytes()).unwrap_or(0),
        compiled.config().listeners.len(),
        compiled.config().virtual_hosts.len(),
        route_count,
        compiled.config().resources.len(),
        compiled.config().upstreams.len(),
        compiled.config().tls_policies.len(),
        revision.skipped.len(),
    );
    Ok(())
}

async fn run_management_plane() -> MainResult<()> {
    let bind_address = std::env::var("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3800".to_owned());
    // Fail closed: the management listener exposes unauthenticated
    // /healthz, /readyz, /livez, and /metrics. A non-loopback bind is only
    // allowed with an explicit operator authorization (for example
    // Kubernetes probes that must reach the pod address).
    if !bind_address_is_loopback(&bind_address)
        && std::env::var("SDKWORK_WEBSERVER_MANAGEMENT_EXPOSE_ALLOWED").is_err()
    {
        return Err(io::Error::other(
            "management listener (health/readiness/metrics) refuses a non-loopback bind; set SDKWORK_WEBSERVER_MANAGEMENT_EXPOSE_ALLOWED=true to authorize it",
        )
        .into());
    }
    let app = build_router()
        .await
        .map_err(|error| io::Error::other(format!("management bootstrap failed: {error}")))?;
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    tracing::info!(address = %bind_address, "management listener started");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// True when the bind host is the IPv4/IPv6 loopback (optionally with a
/// port). DNS names and wildcards are never treated as loopback.
fn bind_address_is_loopback(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map_or(bind, |(host, _)| host);
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn validate_imported_module_webserver_configs() -> MainResult<()> {
    let reports = validate_configured_module_imports().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("imported module webserver validation failed: {error}"),
        )
    })?;
    for report in &reports {
        tracing::info!(
            module_id = %report.import.id,
            profile = %report.profile,
            path = %report.import.path.display(),
            virtual_hosts = report.virtual_host_count,
            upstreams = report.upstream_count,
            unreachable = report.unreachable_upstreams.len(),
            "validated imported module webserver configuration"
        );
    }
    Ok(())
}

fn validate_module_imports_command() -> MainResult<()> {
    let reports = validate_configured_module_imports().map_err(|error| {
        io::Error::new(io::ErrorKind::InvalidInput, error)
    })?;
    if reports.is_empty() {
        println!("no module webserver imports configured");
        return Ok(());
    }
    for report in reports {
        println!(
            "ok module={} profile={} path={} virtualHosts={} upstreams={} unreachable={}",
            report.import.id,
            report.profile,
            report.import.path.display(),
            report.virtual_host_count,
            report.upstream_count,
            report.unreachable_upstreams.len(),
        );
    }
    Ok(())
}

fn config_path(argument: Option<String>) -> MainResult<PathBuf> {
    resolve_webserver_config_path(argument)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message).into())
}

fn print_help() {
    println!(
        "sdkwork-api-webserver-standalone-gateway\n\
         \n\
         Operations:\n\
           serve-management       Start the existing management API (default).\n\
           db-migrate             Run database migration and exit.\n\
           validate <config>      Validate and compile Web Server app config (JSON, TOML, or nginx conf; format auto-detected).\n\
           validate-module-imports  Validate imported sibling-module deployments/webserver/ configs.\n\
           validate-app-shell     Validate the configured standalone PC app shell.\n\
           data-plane <config>    Start HTTP/HTTPS application listeners without a database.\n\
                                  Set SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND to an explicit loopback socket for host health and metrics.\n\
           serve-nginx <path>     Serve stock nginx.conf or a sites-enabled directory (loads companion stream-conf.d).\n\
           validate-nginx <path>  Materialize and validate nginx compatibility without starting listeners.\n\
         \n\
         Format selection: validate and data-plane auto-detect the config\n\
         format by extension, directory layout, or content. Pass\n\
         --format json|toml|nginx (or --format=<format>) to force one.\n\
         \n\
         Config resolution: explicit <config> argument, then\n\
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
