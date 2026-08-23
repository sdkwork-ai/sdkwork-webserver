//! Certificate inventory directory contract (`/etc/sdkwork/certs/<domain>/`)
//! and `certs://<domain>/` reference resolution.

use std::{fs, path::PathBuf};

use sdkwork_webserver_core::{
    canonical_certificate_domain_directory, canonical_certificate_file,
    canonical_certificate_key_file, canonical_certificates_directory, load_and_compile_webserver_config,
};

/// Env-sensitive tests must not run concurrently (shared process env).
static CERTS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_certs_dir(directory: &std::path::Path, test: impl FnOnce()) {
    let _guard = CERTS_ENV_LOCK.lock().expect("lock");
    let previous = std::env::var("SDKWORK_CERTS_DIR").ok();
    std::env::set_var("SDKWORK_CERTS_DIR", directory);
    test();
    match previous {
        Some(value) => std::env::set_var("SDKWORK_CERTS_DIR", value),
        None => std::env::remove_var("SDKWORK_CERTS_DIR"),
    }
}

#[test]
fn canonical_domain_directory_layout() {
    let temp = tempfile::tempdir().expect("temp");
    with_certs_dir(temp.path(), || {
        let root = canonical_certificates_directory().expect("root");
        assert_eq!(root, temp.path().to_path_buf());
        assert_eq!(
            canonical_certificate_domain_directory("SDKWork.COM.").expect("domain"),
            temp.path().join("sdkwork.com")
        );
        assert_eq!(
            canonical_certificate_file("app.sdkwork.com").expect("cert"),
            temp.path().join("app.sdkwork.com/cert.pem")
        );
        assert_eq!(
            canonical_certificate_key_file("app.sdkwork.com").expect("key"),
            temp.path().join("app.sdkwork.com/key.pem")
        );
    });
}

#[test]
fn invalid_domains_are_rejected() {
    with_certs_dir(&PathBuf::from("/tmp"), || {
        assert!(canonical_certificate_domain_directory("").is_err());
        assert!(canonical_certificate_domain_directory("bad domain").is_err());
        assert!(canonical_certificate_domain_directory("../../etc").is_err());
        let long = "a".repeat(254);
        assert!(canonical_certificate_domain_directory(&long).is_err());
    });
}

#[test]
fn certs_uri_references_resolve_to_the_domain_inventory() {
    let temp = tempfile::tempdir().expect("temp");
    let domain_dir = temp.path().join("sdkwork.com");
    fs::create_dir_all(&domain_dir).expect("domain dir");
    fs::write(domain_dir.join("cert.pem"), "cert").expect("cert");
    fs::write(domain_dir.join("key.pem"), "key").expect("key");

    with_certs_dir(temp.path(), || {
        // A config whose certificate source uses `certs://sdkwork.com/…`
        // must compile against the inventory directory.
        let directory = tempfile::tempdir().expect("config temp");
        let config_path = directory.path().join("config.json");
        let config = serde_json::json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "certs-uri-test",
            "limits": { "maxConnections": 16 },
            "listeners": [{
                "id": "https",
                "bind": "127.0.0.1",
                "port": 18443,
                "protocols": ["http1"],
                "tlsPolicyRef": "tls",
                "defaultVirtualHostRef": "host"
            }],
            "certificates": [{
                "id": "site",
                "serverNames": ["sdkwork.com"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "certs://sdkwork.com/cert.pem",
                    "privateKeyFile": "certs://sdkwork.com/key.pem"
                }
            }],
            "tlsPolicies": [{
                "id": "tls",
                "certificateRef": "site",
                "minimumVersion": "tls1.2",
                "maximumVersion": "tls1.3",
                "alpn": ["http/1.1"]
            }],
            "resources": [{
                "id": "ok",
                "type": "respond",
                "status": 200,
                "contentType": "text/plain; charset=utf-8",
                "body": "ok"
            }],
            "virtualHosts": [{
                "id": "host",
                "listenerRefs": ["https"],
                "serverNames": ["sdkwork.com"],
                "routes": [{
                    "id": "ok-route",
                    "match": { "pathType": "prefix", "path": "/" },
                    "resourceRef": "ok"
                }]
            }]
        });
        fs::write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize"))
            .expect("write");
        let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
        let (certificate_file, private_key_file) = compiled
            .certificate_paths("site")
            .expect("resolved certificate paths");
        assert_eq!(
            certificate_file,
            domain_dir.join("cert.pem").canonicalize().expect("canonical")
        );
        assert_eq!(
            private_key_file,
            domain_dir.join("key.pem").canonicalize().expect("canonical")
        );
    });
}

#[test]
fn missing_certs_uri_target_fails_closed() {
    let temp = tempfile::tempdir().expect("temp");
    with_certs_dir(temp.path(), || {
        let directory = tempfile::tempdir().expect("config temp");
        let config_path = directory.path().join("config.json");
        let config = serde_json::json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "certs-uri-missing",
            "listeners": [{
                "id": "http",
                "bind": "127.0.0.1",
                "port": 18444,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "host"
            }],
            "resources": [{
                "id": "ok",
                "type": "respond",
                "status": 200,
                "contentType": "text/plain; charset=utf-8",
                "body": "ok"
            }],
            "virtualHosts": [{
                "id": "host",
                "listenerRefs": ["http"],
                "serverNames": ["gone.example"],
                "routes": [{
                    "id": "ok-route",
                    "match": { "pathType": "prefix", "path": "/" },
                    "resourceRef": "ok"
                }]
            }],
            "certificates": [{
                "id": "missing",
                "serverNames": ["gone.example"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "certs://gone.example/cert.pem",
                    "privateKeyFile": "certs://gone.example/key.pem"
                }
            }]
        });
        fs::write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize"))
            .expect("write");
        let error = load_and_compile_webserver_config(&config_path)
            .expect_err("missing inventory file must fail closed");
        let diagnostics = error
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diagnostics.contains("certificate file"),
            "unexpected diagnostics: {diagnostics}"
        );
    });
}

#[test]
fn docker_standalone_config_compiles_with_8110_and_8430_listeners() {
    let temp = tempfile::tempdir().expect("temp");
    // Provision the canonical inventory for the docker config's domains.
    for domain in ["sdkwork.com", "app.sdkwork.com"] {
        let dir = temp.path().join(domain);
        fs::create_dir_all(&dir).expect("domain dir");
        fs::write(dir.join("cert.pem"), "cert").expect("cert");
        fs::write(dir.join("key.pem"), "key").expect("key");
    }
    with_certs_dir(temp.path(), || {
        let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates")
            .parent()
            .expect("sdkwork-webserver")
            .join("deployments/docker/config/sdkwork.webserver.config.json");
        let compiled = load_and_compile_webserver_config(&config_path).expect("docker config compiles");
        let ports: Vec<u16> = compiled.config().listeners.iter().map(|l| l.port).collect();
        assert!(ports.contains(&8110), "HTTP listener 8110 missing: {ports:?}");
        assert!(ports.contains(&8430), "HTTPS listener 8430 missing: {ports:?}");
        let https = compiled
            .config()
            .listeners
            .iter()
            .find(|listener| listener.port == 8430)
            .expect("https listener");
        assert!(
            https.tls_policy_ref.is_some(),
            "https listener must carry a TLS policy"
        );
        assert_eq!(compiled.config().certificates.len(), 2);
        // Both certificates resolve to the inventory (compile proves it).
        assert!(compiled.certificate_paths("sdkwork-com").is_some());
        assert!(compiled.certificate_paths("app-sdkwork-com").is_some());
    });
}
