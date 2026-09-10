// Temporary diagnostic: load the live cloudrouter sidecar through the import
// loader and dump listener/vhost wiring. Delete after debugging.
use sdkwork_webserver_core::module_imports::{load_module_import_app_config, WebserverModuleImport};

#[test]
fn dump_cloudrouter_materialization() {
    let path = if std::path::Path::new("/tmp/router-sidecar/nginx.cloud.development.conf").exists() {
        "/tmp/router-sidecar/nginx.cloud.development.conf".to_owned()
    } else {
        "../sdkwork-cloudrouter/deployments/webserver/nginx.cloud.development.conf".to_owned()
    };
    let import = WebserverModuleImport {
        id: "sdkwork-cloudrouter".to_owned(),
        path: path.clone().into(),
        profile: Some("cloud".to_owned()),
        enabled: true,
        required: false,
        probe_upstreams: false,
    };
    let app = load_module_import_app_config(&import).expect("load sidecar");
    for listener in &app.listeners {
        println!(
            "LISTENER id={} bind={} port={} default={:?}",
            listener.id,
            listener.bind,
            listener.port,
            listener.default_virtual_host_ref
        );
    }
    for host in &app.virtual_hosts {
        println!(
            "VHOST id={} refs={:?} names={} routes={}",
            host.id,
            host.listener_refs,
            host.server_names.len(),
            host.routes.len()
        );
        for route in host.routes.iter().take(8) {
            println!(
                "  ROUTE id={} match={:?} resource_ref={}",
                route.id,
                route.route_match,
                route.resource_ref
            );
        }
    }
    for resource in &app.resources {
        println!("RESOURCE {}", serde_json::to_string_pretty(resource).unwrap_or_default());
    }
    let loader = sdkwork_webserver_core::WebServerConfigLoader::new();
    let options = sdkwork_webserver_core::ConfigLoadOptions {
        format: Some(sdkwork_webserver_core::ConfigFormat::NginxConf),
        app_key: Some("probe".to_owned()),
        ..Default::default()
    };
    let compiled = loader
        .load_and_compile(std::path::Path::new(&path), &options)
        .expect("load_and_compile");
    for listener_id in ["listener-0-0-0-0-80"] {
        for authority in ["router-dev.sdkwork.com", "server-dev.sdkwork.com", "unknown.example.com"] {
            let selected = compiled.select_route(listener_id, authority, "/", "GET");
            println!(
                "SELECT listener={listener_id} host={authority} -> {}",
                selected.map(|s| s.virtual_host.id.as_str()).unwrap_or("NONE")
            );
        }
    }
}
