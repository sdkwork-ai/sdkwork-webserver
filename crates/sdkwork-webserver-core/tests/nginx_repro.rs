#[test]
fn repro_api_site() {
    use sdkwork_webserver_core::nginx::{materialize_nginx_app, parse_nginx_config};
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/api-repro.conf"))
            .unwrap();
    let parsed = parse_nginx_config(&text, std::path::Path::new("api.conf")).unwrap();
    match materialize_nginx_app(
        &parsed,
        std::path::Path::new("/etc/nginx/sites-enabled/sdkwork"),
        "repro",
    ) {
        Ok(app) => {
            eprintln!(
                "OK vhosts={} listeners={}",
                app.virtual_hosts.len(),
                app.listeners.len()
            );
        }
        Err(error) => {
            eprintln!(
                "ERROR: {error} type={}",
                std::any::type_name::<sdkwork_webserver_core::nginx::NginxConfigError>()
            );
            eprintln!("error type_name={}", std::any::type_name_of_val(&error));
            let direct = std::error::Error::source(&error);
            eprintln!("direct source: {:?}", direct.map(|s| s.to_string()));
            let mut source = direct;
            while let Some(s) = source {
                eprintln!("  source: {s}");
                if let Some(config) =
                    s.downcast_ref::<sdkwork_webserver_core::WebServerConfigError>()
                {
                    for diagnostic in config.diagnostics() {
                        eprintln!("  diag: {}: {}", diagnostic.path, diagnostic.message);
                    }
                }
                source = s.source();
            }
        }
    }
}
