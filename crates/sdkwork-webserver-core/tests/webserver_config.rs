use std::{fs, path::Path};

use sdkwork_webserver_core::{
    inspect_webserver_config_revision, load_and_compile_webserver_config,
    load_and_compile_webserver_config_revision, ConfigProviderType, ProxyProtocolCrc32cPolicy,
    ProxyProtocolVersion, ResourceConfig, ResourceSampleFailurePolicy, TrustedProxyHeader,
    WebServerConfigError,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn base_config() -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "default-host"
        }],
        "resources": [
            {
                "id": "exact-response",
                "type": "respond",
                "status": 200,
                "body": "exact"
            },
            {
                "id": "prefix-response",
                "type": "respond",
                "status": 200,
                "body": "prefix"
            },
            {
                "id": "wildcard-response",
                "type": "respond",
                "status": 200,
                "body": "wildcard"
            }
        ],
        "virtualHosts": [
            {
                "id": "default-host",
                "listenerRefs": ["http"],
                "serverNames": ["example.com"],
                "routes": [
                    {
                        "id": "exact-route",
                        "match": {
                            "pathType": "exact",
                            "path": "/api/status",
                            "methods": ["GET"]
                        },
                        "resourceRef": "exact-response"
                    },
                    {
                        "id": "prefix-route",
                        "match": {
                            "pathType": "prefix",
                            "path": "/api"
                        },
                        "resourceRef": "prefix-response"
                    }
                ]
            },
            {
                "id": "wildcard-host",
                "listenerRefs": ["http"],
                "serverNames": ["*.example.net"],
                "routes": [{
                    "id": "wildcard-route",
                    "match": {
                        "pathType": "prefix",
                        "path": "/"
                    },
                    "resourceRef": "wildcard-response"
                }]
            }
        ]
    })
}

fn write_config(directory: &Path, config: &Value) -> std::path::PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize test config"),
    )
    .expect("write test config");
    path
}

#[test]
fn checked_in_example_validates_and_compiles() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../etc/examples/sdkwork.webserver.config.json");
    let compiled = load_and_compile_webserver_config(path).expect("compile checked-in example");
    assert_eq!(compiled.config().limits.max_connection_age_ms, 3_600_000);
    let pressure = compiled
        .config()
        .deployment
        .resource_pressure
        .as_ref()
        .expect("checked-in example enables resource-pressure admission");
    assert_eq!(pressure.operations_reserve_requests, 16);
    assert_eq!(
        pressure.sample_failure_policy,
        ResourceSampleFailurePolicy::FailClosed
    );

    let selected = compiled
        .select_route("public-http", "LOCALHOST:8080", "/healthz", "GET")
        .expect("select health route");
    assert_eq!(selected.virtual_host.id, "example-web");
    assert_eq!(selected.route.id, "health");
    assert!(matches!(selected.resource, ResourceConfig::Respond { .. }));
}

#[test]
fn configuration_revision_is_stable_for_exact_bytes_and_changes_with_content() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    let path = write_config(directory.path(), &config);

    let first = load_and_compile_webserver_config_revision(&path).expect("compile first revision");
    let second =
        load_and_compile_webserver_config_revision(&path).expect("compile identical revision");
    let inspected = inspect_webserver_config_revision(&path).expect("inspect identical revision");
    assert_eq!(first.sha256(), second.sha256());
    assert_eq!(first.sha256(), inspected.sha256());
    assert_eq!(first.size_bytes(), inspected.size_bytes());
    assert_eq!(first.sha256().len(), 64);
    assert!(first.size_bytes() > 0);

    config["resources"][0]["body"] = json!("changed");
    write_config(directory.path(), &config);
    let changed =
        load_and_compile_webserver_config_revision(&path).expect("compile changed revision");
    assert_ne!(first.sha256(), changed.sha256());
}

#[test]
fn exact_route_precedes_prefix_and_methods_are_enforced() {
    let directory = TempDir::new().expect("create temp directory");
    let path = write_config(directory.path(), &base_config());
    let compiled = load_and_compile_webserver_config(path).expect("compile config");

    let get = compiled
        .select_route("http", "example.com", "/api/status", "GET")
        .expect("select exact GET route");
    assert_eq!(get.route.id, "exact-route");

    let post = compiled
        .select_route("http", "example.com", "/api/status", "POST")
        .expect("fall back to prefix POST route");
    assert_eq!(post.route.id, "prefix-route");

    let wildcard = compiled
        .select_route("http", "api.example.net:18080", "/", "GET")
        .expect("select wildcard host");
    assert_eq!(wildcard.virtual_host.id, "wildcard-host");
    assert!(compiled
        .select_route("http", "api.eu.example.net:18080", "/", "GET")
        .is_none());
}

#[test]
fn schema_rejects_unknown_fields() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["pretendTls"] = json!(true);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("unknown field must fail");
    assert!(matches!(error, WebServerConfigError::Validation { .. }));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("pretendTls")));
}

#[test]
fn trusted_proxy_contract_defaults_to_bounded_nginx_compatible_behavior() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["trustedProxy"] = json!({
        "trustedCidrs": ["127.0.0.0/8", "10.0.0.0/8"]
    });
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile trusted proxy policy");
    let policy = compiled
        .listener("http")
        .and_then(|listener| listener.trusted_proxy.as_ref())
        .expect("compiled trusted proxy policy");
    assert_eq!(policy.header, TrustedProxyHeader::XForwardedFor);
    assert!(!policy.recursive);
    assert_eq!(policy.max_hops, 16);
    assert_eq!(policy.max_header_bytes, 4_096);
}

#[test]
fn trusted_proxy_schema_rejects_ambiguous_or_unbounded_policy() {
    let invalid_policies = [
        json!({"trustedCidrs": []}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "header": "forwarded"}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "maxHops": 0}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "maxHops": 65}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "maxHeaderBytes": 63}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "maxHeaderBytes": 65_537}),
        json!({"trustedCidrs": ["not-a-cidr"]}),
        json!({"trustedCidrs": ["127.0.0.0/8"], "trustAll": true}),
        json!({"trustedCidrs": ["0.0.0.0/0"]}),
        json!({"trustedCidrs": ["0.0.0.0/7"]}),
        json!({"trustedCidrs": ["::/0"]}),
        json!({"trustedCidrs": ["::/15"]}),
        json!({"trustedCidrs": ["::ffff:0:0/96"]}),
    ];

    for (index, policy) in invalid_policies.into_iter().enumerate() {
        let directory = TempDir::new().expect("create temp directory");
        let mut config = base_config();
        config["listeners"][0]["trustedProxy"] = policy;
        let path = write_config(directory.path(), &config);
        let error = match load_and_compile_webserver_config(path) {
            Ok(_) => panic!("invalid trusted proxy policy {index} must fail"),
            Err(error) => error,
        };
        assert!(
            !error.to_string().is_empty(),
            "invalid trusted proxy policy {index} must return a diagnostic"
        );
    }
}

#[test]
fn trusted_proxy_header_budget_cannot_exceed_the_global_field_budget() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"maxHeaderValueBytes": 128});
    config["listeners"][0]["trustedProxy"] = json!({
        "trustedCidrs": ["127.0.0.0/8"],
        "maxHeaderBytes": 256
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("incoherent budget must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path == "/listeners/0/trustedProxy/maxHeaderBytes"
            && diagnostic.message.contains("maxHeaderValueBytes")
    }));
}

#[test]
fn proxy_protocol_contract_is_mandatory_trusted_bounded_and_versioned() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["proxyProtocol"] = json!({
        "trustedSourceCidrs": ["127.0.0.0/8"]
    });
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile PROXY policy");
    let policy = compiled
        .listener("http")
        .and_then(|listener| listener.proxy_protocol.as_ref())
        .expect("compiled PROXY policy");
    assert_eq!(
        policy.versions,
        [ProxyProtocolVersion::V1, ProxyProtocolVersion::V2]
    );
    assert_eq!(policy.timeout_ms, 3_000);
    assert_eq!(policy.max_header_bytes, 536);
    assert_eq!(policy.crc32c_policy, ProxyProtocolCrc32cPolicy::Ignore);
}

#[test]
fn proxy_protocol_schema_and_semantics_reject_unsafe_or_ambiguous_policy() {
    let invalid = [
        json!({"trustedSourceCidrs": []}),
        json!({"trustedSourceCidrs": ["not-a-cidr"]}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "versions": []}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "versions": ["v3"]}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "timeoutMs": 99}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "timeoutMs": 10_001}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "maxHeaderBytes": 106}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "maxHeaderBytes": 4_097}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "crc32cPolicy": "unknown"}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "versions": ["v1"], "crc32cPolicy": "validate-if-present"}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "versions": ["v1"], "crc32cPolicy": "required"}),
        json!({"trustedSourceCidrs": ["127.0.0.0/8"], "optional": true}),
    ];
    for (index, policy) in invalid.into_iter().enumerate() {
        let directory = TempDir::new().expect("create temp directory");
        let mut config = base_config();
        config["listeners"][0]["proxyProtocol"] = policy;
        let path = write_config(directory.path(), &config);
        assert!(
            load_and_compile_webserver_config(path).is_err(),
            "invalid PROXY policy {index} must fail"
        );
    }

    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["proxyProtocol"] = json!({
        "trustedSourceCidrs": ["127.0.0.0/8"]
    });
    config["listeners"][0]["trustedProxy"] = json!({
        "trustedCidrs": ["127.0.0.0/8"]
    });
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(path).expect_err("identity authorities conflict");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path == "/listeners/0/proxyProtocol"
            && diagnostic.message.contains("mutually exclusive")
    }));
}

#[test]
fn resource_pressure_defaults_are_finite_and_compile() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["deployment"] = json!({"resourcePressure": {}});
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile default policy");
    let policy = compiled
        .config()
        .deployment
        .resource_pressure
        .as_ref()
        .expect("resource pressure policy");
    assert_eq!(policy.sample_interval_ms, 250);
    assert_eq!(policy.maximum_process_memory_bytes, 1_073_741_824);
    assert_eq!(policy.memory_reserve_bytes, 67_108_864);
    assert_eq!(policy.memory_admission_percent, 90);
    assert_eq!(policy.memory_recovery_percent, 80);
    assert_eq!(policy.maximum_open_handles, 16_384);
    assert_eq!(policy.open_handle_reserve, 128);
    assert_eq!(policy.open_handle_admission_percent, 90);
    assert_eq!(policy.open_handle_recovery_percent, 80);
    assert_eq!(policy.event_loop_lag_admission_ms, 250);
    assert_eq!(policy.event_loop_lag_recovery_ms, 50);
    assert_eq!(policy.consecutive_pressure_samples, 2);
    assert_eq!(policy.consecutive_recovery_samples, 4);
    assert_eq!(policy.operations_reserve_requests, 16);
    assert_eq!(
        policy.sample_failure_policy,
        ResourceSampleFailurePolicy::FailClosed
    );
}

#[test]
fn resource_pressure_schema_rejects_every_numeric_bound() {
    let directory = TempDir::new().expect("create temp directory");
    let cases = [
        ("sampleIntervalMs", json!(49)),
        ("sampleIntervalMs", json!(10_001)),
        ("maximumProcessMemoryBytes", json!(134_217_727_u64)),
        ("maximumProcessMemoryBytes", json!(17_592_186_044_417_u64)),
        ("memoryReserveBytes", json!(16_777_215_u64)),
        ("memoryReserveBytes", json!(68_719_476_737_u64)),
        ("memoryAdmissionPercent", json!(49)),
        ("memoryAdmissionPercent", json!(100)),
        ("memoryRecoveryPercent", json!(0)),
        ("memoryRecoveryPercent", json!(99)),
        ("maximumOpenHandles", json!(63)),
        ("maximumOpenHandles", json!(1_048_577)),
        ("openHandleReserve", json!(7)),
        ("openHandleReserve", json!(65_537)),
        ("openHandleAdmissionPercent", json!(49)),
        ("openHandleAdmissionPercent", json!(100)),
        ("openHandleRecoveryPercent", json!(0)),
        ("openHandleRecoveryPercent", json!(99)),
        ("eventLoopLagAdmissionMs", json!(9)),
        ("eventLoopLagAdmissionMs", json!(10_001)),
        ("eventLoopLagRecoveryMs", json!(0)),
        ("eventLoopLagRecoveryMs", json!(10_000)),
        ("consecutivePressureSamples", json!(0)),
        ("consecutivePressureSamples", json!(101)),
        ("consecutiveRecoverySamples", json!(0)),
        ("consecutiveRecoverySamples", json!(101)),
        ("operationsReserveRequests", json!(0)),
        ("operationsReserveRequests", json!(1_025)),
    ];

    for (field, invalid) in cases {
        let mut config = base_config();
        config["deployment"] = json!({"resourcePressure": {(field): invalid}});
        let path = write_config(directory.path(), &config);
        let error = load_and_compile_webserver_config(path)
            .expect_err("out-of-bound resource pressure value must fail");
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.path.contains(field)),
            "missing diagnostic for {field}: {:?}",
            error.diagnostics()
        );
    }
}

#[test]
fn resource_pressure_rejects_incoherent_reserves_thresholds_and_request_partition() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"maxConcurrentRequests": 1024});
    config["deployment"] = json!({
        "resourcePressure": {
            "maximumProcessMemoryBytes": 134_217_728,
            "memoryReserveBytes": 134_217_728,
            "memoryAdmissionPercent": 80,
            "memoryRecoveryPercent": 80,
            "maximumOpenHandles": 128,
            "openHandleReserve": 128,
            "openHandleAdmissionPercent": 80,
            "openHandleRecoveryPercent": 80,
            "eventLoopLagAdmissionMs": 100,
            "eventLoopLagRecoveryMs": 100,
            "operationsReserveRequests": 1024
        }
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("incoherent resource pressure policy must fail");
    for field in [
        "memoryReserveBytes",
        "memoryRecoveryPercent",
        "openHandleReserve",
        "openHandleRecoveryPercent",
        "eventLoopLagRecoveryMs",
        "operationsReserveRequests",
    ] {
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.path.contains(field)),
            "missing diagnostic for {field}: {:?}",
            error.diagnostics()
        );
    }
}

#[test]
fn resource_pressure_rejects_effective_thresholds_collapsed_by_reserves() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["deployment"] = json!({
        "resourcePressure": {
            "maximumProcessMemoryBytes": 134_217_728,
            "memoryReserveBytes": 67_108_864,
            "memoryAdmissionPercent": 90,
            "memoryRecoveryPercent": 80,
            "maximumOpenHandles": 128,
            "openHandleReserve": 64,
            "openHandleAdmissionPercent": 90,
            "openHandleRecoveryPercent": 80
        }
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("reserve-collapsed hysteresis must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.contains("memoryReserveBytes")
            && diagnostic.message.contains("effective memory recovery")
    }));
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.contains("openHandleReserve")
            && diagnostic
                .message
                .contains("effective open-handle recovery")
    }));
}

#[test]
fn resource_pressure_rejects_unknown_fields_and_failure_policies() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["deployment"] = json!({
        "resourcePressure": {
            "pretendAllocatorLimit": true,
            "sampleFailurePolicy": "best-effort"
        }
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("unknown resource pressure controls must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("pretendAllocatorLimit")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("sampleFailurePolicy")));
}

#[test]
fn schema_rejects_unknown_path_type_and_unbounded_limits() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["routes"][0]["match"]["pathType"] = json!("glob");
    config["limits"] = json!({"maxConnections": 1_000_001});
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("unsupported route modes and unbounded limits must fail");
    assert!(matches!(error, WebServerConfigError::Validation { .. }));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("pathType")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("maxConnections")));
}

#[test]
fn schema_accepts_limit_req_zones_and_regex_routes() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limitReqZones"] = json!([{
        "name": "one",
        "key": "$binary_remote_addr",
        "maxKeys": 160000,
        "ratePerSecond": 1.0
    }]);
    config["virtualHosts"][0]["routes"] = json!([
        {
            "id": "limited",
            "match": { "pathType": "regex-ignore-case", "path": "\\.HTML$" },
            "resourceRef": "exact-response",
            "limitReq": [{ "zone": "one", "burst": 5, "nodelay": true }],
            "rewrite": [{
                "pattern": "^/old$",
                "replacement": "/new",
                "flag": "permanent"
            }]
        },
        {
            "id": "root",
            "match": { "pathType": "prefix", "path": "/" },
            "resourceRef": "prefix-response"
        }
    ]);
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("limitReqZones + regex must compile");
    assert_eq!(compiled.config().limit_req_zones.len(), 1);
    assert_eq!(compiled.config().limit_req_zones[0].name, "one");
    let selected = compiled
        .select_route("http", "example.com", "/index.HTML", "GET")
        .expect("case-insensitive regex route");
    assert_eq!(selected.route.id, "limited");
    assert_eq!(selected.route.rewrite[0].flag, sdkwork_webserver_core::RewriteFlag::Permanent);
}

#[test]
fn regex_and_prefix_exclusive_routes_follow_nginx_selection() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["routes"] = json!([
        {
            "id": "exclusive",
            "match": { "pathType": "prefix-exclusive", "path": "/assets/" },
            "resourceRef": "exact-response"
        },
        {
            "id": "regex-php",
            "match": { "pathType": "regex", "path": "\\.php$" },
            "resourceRef": "prefix-response"
        },
        {
            "id": "prefix",
            "match": { "pathType": "prefix", "path": "/" },
            "resourceRef": "wildcard-response"
        }
    ]);
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile config");

    let exclusive = compiled
        .select_route("http", "example.com", "/assets/app.php", "GET")
        .expect("exclusive prefix suppresses regex");
    assert_eq!(exclusive.route.id, "exclusive");

    let regex = compiled
        .select_route("http", "example.com", "/page.php", "GET")
        .expect("regex wins over plain prefix");
    assert_eq!(regex.route.id, "regex-php");

    let prefix = compiled
        .select_route("http", "example.com", "/other", "GET")
        .expect("prefix fallback");
    assert_eq!(prefix.route.id, "prefix");
}

#[test]
fn schema_rejects_invalid_response_progress_timeouts() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "responseBodyIdleTimeoutMs": 99,
        "connectionWriteTimeoutMs": 3_600_001
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("response progress timeouts must stay within finite bounds");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("responseBodyIdleTimeoutMs")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("connectionWriteTimeoutMs")));
}

#[test]
fn schema_rejects_invalid_request_body_progress_timeouts() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "requestBodyStartTimeoutMs": 99,
        "requestBodyIdleTimeoutMs": 3_600_001
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("request Body progress timeouts must stay within finite bounds");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("requestBodyStartTimeoutMs")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("requestBodyIdleTimeoutMs")));
}

#[test]
fn schema_rejects_invalid_http1_keep_alive_idle_timeout() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"http1KeepAliveIdleTimeoutMs": 99});
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("HTTP/1 Keep-Alive idle timeout must stay within finite bounds");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("http1KeepAliveIdleTimeoutMs")));
}

#[test]
fn schema_rejects_invalid_http1_pipeline_depth() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"http1MaxPipelineDepth": 0});
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("zero HTTP/1 Pipeline depth must fail schema validation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("http1MaxPipelineDepth")));
}

#[test]
fn schema_rejects_connection_maximum_age_outside_the_finite_range() {
    for value in [99_u64, 86_400_001_u64] {
        let directory = TempDir::new().expect("create temp directory");
        let mut config = base_config();
        config["limits"] = json!({"maxConnectionAgeMs": value});
        let path = write_config(directory.path(), &config);

        let error = load_and_compile_webserver_config(path)
            .expect_err("connection maximum age outside the finite range must fail");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.path.contains("maxConnectionAgeMs")));
    }
}

#[test]
fn upstream_address_policy_rejects_private_literals_and_broad_cidrs() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resolvers"] = json!([{
        "id": "system-dns",
        "servers": ["8.8.8.8"],
        "timeoutMs": 2_000,
        "maximumAnswers": 16,
        "maxConcurrentQueries": 64
    }]);
    config["upstreams"] = json!([{
        "id": "private-upstream",
        "resolverRef": "missing-resolver",
        "addressPolicy": {"allowedCidrs": ["0.0.0.0/0"]},
        "targets": [{"url": "http://127.0.0.1:8080"}]
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("unsafe resolver and address policy must fail compilation");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.contains("/resolvers/0/servers")
            && diagnostic.message.contains("not implemented")
    }));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("resolverRef")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("allowedCidrs")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("literal upstream IP is forbidden")));
}

#[test]
fn upstream_address_policy_allows_explicit_narrow_loopback_cidr() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resolvers"] = json!([{
        "id": "system-dns",
        "timeoutMs": 2_000,
        "maximumAnswers": 16,
        "maxConcurrentQueries": 64
    }]);
    config["upstreams"] = json!([{
        "id": "local-upstream",
        "resolverRef": "system-dns",
        "addressPolicy": {"allowedCidrs": ["127.0.0.1/32"]},
        "idleConnectionTimeoutMs": 1_000,
        "targets": [{"url": "http://127.0.0.1:8080"}]
    }]);
    let path = write_config(directory.path(), &config);

    load_and_compile_webserver_config(path).expect("explicit narrow loopback policy must compile");
}

#[test]
fn schema_rejects_unbounded_resolver_controls() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resolvers"] = json!([{
        "id": "system-dns",
        "maxConcurrentQueries": 1_025
    }]);
    config["upstreams"] = json!([{
        "id": "invalid-upstream",
        "addressPolicy": {"allowedCidrs": ["127.0.0.1/32"]},
        "idleConnectionTimeoutMs": 99,
        "targets": [{"url": "https://example.com"}]
    }]);
    let path = write_config(directory.path(), &config);

    let error =
        load_and_compile_webserver_config(path).expect_err("unbounded resolver controls must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("maxConcurrentQueries")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("idleConnectionTimeoutMs")));
}

#[test]
fn configuration_rejects_malformed_allowed_cidr() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "invalid-upstream",
        "addressPolicy": {"allowedCidrs": ["not-a-cidr"]},
        "targets": [{"url": "https://example.com"}]
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("malformed allowed CIDR must fail deserialization");
    assert!(matches!(error, WebServerConfigError::Json { .. }));
}

#[test]
fn upstream_tls_policy_rejects_plaintext_incoherent_trust_and_incomplete_identity() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "invalid-tls-upstream",
        "targets": [{"url": "http://example.com"}],
        "tls": {
            "trustMode": "system",
            "caCertificateFiles": ["ca.pem"],
            "clientCertificateFile": "client.pem",
            "minimumVersion": "tls1.3",
            "maximumVersion": "tls1.2"
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("incoherent upstream TLS policy must fail semantic validation");
    let diagnostics = error.diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.path == "/upstreams/0/tls"
            && diagnostic.message.contains("every target to use https")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.path.ends_with("/tls/caCertificateFiles")
            && diagnostic.message.contains("system trust mode")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.path == "/upstreams/0/tls" && diagnostic.message.contains("configured together")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.path.ends_with("/tls/maximumVersion")
            && diagnostic.message.contains("must not be lower")
    }));
}

#[test]
fn upstream_tls_custom_trust_requires_safe_existing_files() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "private-upstream",
        "targets": [{"url": "https://example.com"}],
        "tls": {
            "trustMode": "custom",
            "caCertificateFiles": ["../outside-ca.pem"]
        }
    }]);
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(&path)
        .expect_err("parent traversal in upstream CA path must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("caCertificateFiles/0")));

    config["upstreams"][0]["tls"]["caCertificateFiles"] = json!(["missing-ca.pem"]);
    write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(&path)
        .expect_err("missing upstream CA file must fail compilation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("caCertificateFiles/0")));

    fs::write(directory.path().join("private-ca.pem"), "test CA material")
        .expect("write test CA file");
    config["upstreams"][0]["tls"]["caCertificateFiles"] = json!(["private-ca.pem"]);
    write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(&path)
        .expect("safe existing upstream CA path must compile");
    let paths = compiled
        .upstream_tls_ca_certificate_paths("private-upstream")
        .expect("compiled upstream TLS paths");
    assert_eq!(paths.len(), 1);
    assert!(paths[0].starts_with(
        fs::canonicalize(directory.path()).expect("canonical test configuration directory")
    ));
}

#[test]
fn upstream_tls_schema_bounds_custom_ca_files() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "bounded-tls-upstream",
        "targets": [{"url": "https://example.com"}],
        "tls": {
            "trustMode": "custom",
            "caCertificateFiles": (0..9).map(|index| format!("ca-{index}.pem")).collect::<Vec<_>>()
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("more than eight upstream CA files must fail schema validation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("caCertificateFiles")));
}

#[test]
fn upstream_tls_custom_trust_modes_require_ca_files() {
    for trust_mode in ["custom", "system-and-custom"] {
        let directory = TempDir::new().expect("create temp directory");
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "missing-ca-upstream",
            "targets": [{"url": "https://example.com"}],
            "tls": {"trustMode": trust_mode}
        }]);
        let path = write_config(directory.path(), &config);

        let error = load_and_compile_webserver_config(path)
            .expect_err("custom trust without CA files must fail");
        assert!(error.diagnostics().iter().any(|diagnostic| {
            diagnostic.path.ends_with("/tls/caCertificateFiles")
                && diagnostic.message.contains("require at least one")
        }));
    }
}

#[test]
fn upstream_admission_and_passive_health_defaults_are_finite() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "bounded-upstream",
        "targets": [{"url": "https://example.com"}]
    }]);
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile bounded upstream");
    let upstream = &compiled.config().upstreams[0];
    assert_eq!(
        upstream.load_balancing,
        sdkwork_webserver_core::UpstreamLoadBalancingStrategy::RoundRobin
    );
    assert_eq!(upstream.max_connections, 256);
    assert_eq!(upstream.max_idle_connections, 128);
    assert_eq!(upstream.max_response_header_bytes, 64 * 1024);
    assert_eq!(upstream.max_response_headers, 100);
    assert_eq!(upstream.max_in_flight_requests, 1_024);
    assert!(upstream.retry.is_none());
    assert_eq!(upstream.passive_health.failure_threshold, 3);
    assert_eq!(upstream.passive_health.ejection_time_ms, 30_000);
    assert_eq!(upstream.passive_health.failure_statuses, [502, 503, 504]);
    assert!(upstream.active_health.is_none());
    assert_eq!(compiled.config().limits.max_concurrent_health_checks, 64);
}

#[test]
fn upstream_load_balancing_strategy_is_typed_and_defaults_compatibly() {
    use sdkwork_webserver_core::UpstreamLoadBalancingStrategy;

    let directory = TempDir::new().expect("create temp directory");
    let mut configured = base_config();
    configured["upstreams"] = json!([
        {
            "id": "round-robin-upstream",
            "targets": [{"url": "https://round-robin.example.com"}],
            "loadBalancing": "round-robin"
        },
        {
            "id": "least-connections-upstream",
            "targets": [{"url": "https://least-connections.example.com"}],
            "loadBalancing": "least-connections"
        },
        {
            "id": "random-two-upstream",
            "targets": [
                {"url": "https://random-two-a.example.com"},
                {"url": "https://random-two-b.example.com"}
            ],
            "loadBalancing": "random-two-least-connections"
        },
        {
            "id": "ip-hash-upstream",
            "targets": [
                {"url": "https://ip-hash-a.example.com"},
                {"url": "https://ip-hash-b.example.com"}
            ],
            "loadBalancing": "ip-hash"
        },
        {
            "id": "default-upstream",
            "targets": [{"url": "https://default.example.com"}]
        }
    ]);
    let path = write_config(directory.path(), &configured);
    let compiled = load_and_compile_webserver_config(path)
        .expect("supported load-balancing strategies must compile");
    assert_eq!(
        compiled.config().upstreams[0].load_balancing,
        UpstreamLoadBalancingStrategy::RoundRobin
    );
    assert_eq!(
        compiled.config().upstreams[1].load_balancing,
        UpstreamLoadBalancingStrategy::LeastConnections
    );
    assert_eq!(
        compiled.config().upstreams[2].load_balancing,
        UpstreamLoadBalancingStrategy::RandomTwoLeastConnections
    );
    assert_eq!(
        compiled.config().upstreams[3].load_balancing,
        UpstreamLoadBalancingStrategy::IpHash
    );
    assert_eq!(
        compiled.config().upstreams[4].load_balancing,
        UpstreamLoadBalancingStrategy::RoundRobin
    );

    for invalid in [
        json!("least_conn"),
        json!("random"),
        json!("random two least_conn"),
        json!("random-two"),
        json!("ip_hash"),
        json!("ip hash"),
        json!(true),
        json!(1),
    ] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "invalid-strategy-upstream",
            "targets": [{"url": "https://example.com"}],
            "loadBalancing": invalid
        }]);
        let path = write_config(directory.path(), &config);
        load_and_compile_webserver_config(path)
            .expect_err("unknown or non-string load-balancing strategy must fail");
    }

    let mut alias = base_config();
    alias["upstreams"] = json!([{
        "id": "invalid-strategy-upstream",
        "targets": [{"url": "https://example.com"}],
        "strategy": "least-connections"
    }]);
    let path = write_config(directory.path(), &alias);
    load_and_compile_webserver_config(path).expect_err("unknown load-balancing alias must fail");

    let mut incompatible = base_config();
    incompatible["upstreams"] = json!([{
        "id": "incompatible-ip-hash-upstream",
        "targets": [
            {"url": "https://one.example.com", "slowStartMs": 1_000},
            {"url": "https://two.example.com"}
        ],
        "loadBalancing": "ip-hash"
    }]);
    let path = write_config(directory.path(), &incompatible);
    let error = load_and_compile_webserver_config(path)
        .expect_err("Nginx-incompatible ip-hash slow start must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.ends_with("/loadBalancing") && diagnostic.message.contains("slowStartMs")
    }));
}

#[test]
fn upstream_retry_contract_is_opt_in_bounded_and_coherent() {
    let directory = TempDir::new().expect("create temp directory");
    let mut valid = base_config();
    valid["upstreams"] = json!([{
        "id": "retry-upstream",
        "targets": [
            {"url": "https://one.example.com"},
            {"url": "https://two.example.com"},
            {"url": "https://three.example.com"}
        ],
        "requestTimeoutMs": 1_000,
        "retry": {
            "maxAttempts": 3,
            "timeoutMs": 2_000,
            "retryOn": ["error", "timeout", "http_503"]
        }
    }]);
    let path = write_config(directory.path(), &valid);
    let compiled = load_and_compile_webserver_config(path).expect("compile retry policy");
    let retry = compiled.config().upstreams[0]
        .retry
        .as_ref()
        .expect("retry policy");
    assert_eq!(retry.max_attempts, 3);
    assert_eq!(retry.timeout_ms, 2_000);
    assert_eq!(retry.retry_on.len(), 3);

    for retry in [
        json!({"maxAttempts": 1, "timeoutMs": 100, "retryOn": ["timeout"]}),
        json!({"maxAttempts": 9, "timeoutMs": 100, "retryOn": ["timeout"]}),
        json!({"maxAttempts": 2, "timeoutMs": 99, "retryOn": ["timeout"]}),
        json!({"maxAttempts": 2, "timeoutMs": 100, "retryOn": []}),
        json!({"maxAttempts": 2, "timeoutMs": 100, "retryOn": ["timeout", "timeout"]}),
        json!({"maxAttempts": 2, "timeoutMs": 100, "retryOn": ["http_500"]}),
    ] {
        let mut invalid = base_config();
        invalid["upstreams"] = json!([{
            "id": "retry-upstream",
            "targets": [
                {"url": "https://one.example.com"},
                {"url": "https://two.example.com"}
            ],
            "retry": retry
        }]);
        let path = write_config(directory.path(), &invalid);
        load_and_compile_webserver_config(path).expect_err("invalid retry policy must fail");
    }

    let mut too_many_attempts = base_config();
    too_many_attempts["upstreams"] = json!([{
        "id": "retry-upstream",
        "targets": [
            {"url": "https://one.example.com"},
            {"url": "https://two.example.com"}
        ],
        "retry": {"maxAttempts": 3, "timeoutMs": 100, "retryOn": ["timeout"]}
    }]);
    let path = write_config(directory.path(), &too_many_attempts);
    let error = load_and_compile_webserver_config(path)
        .expect_err("attempt count above target count must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.ends_with("/retry/maxAttempts")
            && diagnostic.message.contains("number of upstream targets")
    }));

    let mut unreachable_timeout = base_config();
    unreachable_timeout["upstreams"] = json!([{
        "id": "retry-upstream",
        "targets": [
            {"url": "https://one.example.com"},
            {"url": "https://two.example.com"}
        ],
        "requestTimeoutMs": 100,
        "retry": {"maxAttempts": 2, "timeoutMs": 201, "retryOn": ["timeout"]}
    }]);
    let path = write_config(directory.path(), &unreachable_timeout);
    let error = load_and_compile_webserver_config(path)
        .expect_err("retry timeout above useful attempt budget must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.ends_with("/retry/timeoutMs")
            && diagnostic.message.contains("requestTimeoutMs")
    }));
}

#[test]
fn upstream_response_header_bounds_are_strict() {
    let directory = TempDir::new().expect("create temp directory");
    for (field, invalid_values) in [
        ("maxResponseHeaderBytes", [json!(8_191), json!(1_048_577)]),
        ("maxResponseHeaders", [json!(0), json!(1_025)]),
    ] {
        for invalid in invalid_values {
            let mut config = base_config();
            config["upstreams"] = json!([{
                "id": "bounded-upstream",
                "targets": [{"url": "https://example.com"}],
                field: invalid
            }]);
            let path = write_config(directory.path(), &config);
            let error = load_and_compile_webserver_config(path)
                .expect_err("out-of-bound response Header budget must fail");
            assert!(error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.path.contains(field)));
        }
    }

    let mut exact_boundaries = base_config();
    exact_boundaries["upstreams"] = json!([
        {
            "id": "minimum-header-budget",
            "targets": [{"url": "https://example.com"}],
            "maxResponseHeaderBytes": 8_192,
            "maxResponseHeaders": 1
        },
        {
            "id": "maximum-header-budget",
            "targets": [{"url": "https://example.net"}],
            "maxResponseHeaderBytes": 1_048_576,
            "maxResponseHeaders": 1_024
        }
    ]);
    let path = write_config(directory.path(), &exact_boundaries);
    let compiled = load_and_compile_webserver_config(path)
        .expect("exact response Header budget boundaries must compile");
    assert_eq!(
        compiled.config().upstreams[0].max_response_header_bytes,
        8_192
    );
    assert_eq!(compiled.config().upstreams[0].max_response_headers, 1);
    assert_eq!(
        compiled.config().upstreams[1].max_response_header_bytes,
        1_048_576
    );
    assert_eq!(compiled.config().upstreams[1].max_response_headers, 1_024);

    for alias in ["responseHeaderLimit", "maxUpstreamHeaders"] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "bounded-upstream",
            "targets": [{"url": "https://example.com"}],
            alias: 100
        }]);
        let path = write_config(directory.path(), &config);
        let error = load_and_compile_webserver_config(path)
            .expect_err("unknown response Header budget alias must fail");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains(alias)));
    }
}

#[test]
fn upstream_target_weight_contract_is_complete_and_strict() {
    let directory = TempDir::new().expect("create temp directory");
    let mut weighted = base_config();
    weighted["upstreams"] = json!([{
        "id": "weighted-upstream",
        "targets": [
            {"url": "https://primary.example.com", "weight": 1},
            {"url": "https://secondary.example.com", "weight": 1_000},
            {"url": "https://default.example.com"}
        ]
    }]);
    let path = write_config(directory.path(), &weighted);
    let compiled = load_and_compile_webserver_config(path)
        .expect("exact target weight boundaries must compile");
    assert_eq!(compiled.config().upstreams[0].targets[0].weight, 1);
    assert_eq!(compiled.config().upstreams[0].targets[1].weight, 1_000);
    assert_eq!(compiled.config().upstreams[0].targets[2].weight, 1);

    for invalid in [json!(0), json!(1_001)] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "weighted-upstream",
            "targets": [{"url": "https://example.com", "weight": invalid}]
        }]);
        let path = write_config(directory.path(), &config);
        let error = load_and_compile_webserver_config(path)
            .expect_err("out-of-range target weight must fail");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.path.contains("weight")));
    }

    let mut alias = base_config();
    alias["upstreams"] = json!([{
        "id": "weighted-upstream",
        "targets": [{"url": "https://example.com", "ratio": 3}]
    }]);
    let path = write_config(directory.path(), &alias);
    let error =
        load_and_compile_webserver_config(path).expect_err("unknown target weight alias must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("ratio")));
}

#[test]
fn upstream_backup_target_contract_is_typed_and_requires_a_primary() {
    let directory = TempDir::new().expect("create temp directory");
    let mut tiered = base_config();
    tiered["upstreams"] = json!([{
        "id": "tiered-upstream",
        "targets": [
            {"url": "https://primary.example.com"},
            {"url": "https://backup.example.com", "backup": true}
        ]
    }]);
    let path = write_config(directory.path(), &tiered);
    let compiled = load_and_compile_webserver_config(path)
        .expect("one primary and one backup target must compile");
    assert!(!compiled.config().upstreams[0].targets[0].backup);
    assert!(compiled.config().upstreams[0].targets[1].backup);

    let mut all_backup = base_config();
    all_backup["upstreams"] = json!([{
        "id": "invalid-tiered-upstream",
        "targets": [
            {"url": "https://backup-a.example.com", "backup": true},
            {"url": "https://backup-b.example.com", "backup": true}
        ]
    }]);
    let path = write_config(directory.path(), &all_backup);
    let error = load_and_compile_webserver_config(path)
        .expect_err("an all-backup upstream has no normal traffic tier");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("non-backup primary")));

    let mut non_boolean = base_config();
    non_boolean["upstreams"] = json!([{
        "id": "invalid-tiered-upstream",
        "targets": [{"url": "https://example.com", "backup": "yes"}]
    }]);
    let path = write_config(directory.path(), &non_boolean);
    load_and_compile_webserver_config(path).expect_err("backup must be a JSON boolean");
}

#[test]
fn upstream_target_slow_start_is_optional_bounded_and_strict() {
    let directory = TempDir::new().expect("create temp directory");
    let mut valid = base_config();
    valid["upstreams"] = json!([{
        "id": "slow-start-upstream",
        "targets": [
            {"url": "https://minimum.example.com", "slowStartMs": 100},
            {"url": "https://maximum.example.com", "slowStartMs": 3_600_000},
            {"url": "https://disabled.example.com"}
        ]
    }]);
    let path = write_config(directory.path(), &valid);
    let compiled = load_and_compile_webserver_config(path)
        .expect("exact slow-start boundaries and omission must compile");
    assert_eq!(
        compiled.config().upstreams[0].targets[0].slow_start_ms,
        Some(100)
    );
    assert_eq!(
        compiled.config().upstreams[0].targets[1].slow_start_ms,
        Some(3_600_000)
    );
    assert_eq!(
        compiled.config().upstreams[0].targets[2].slow_start_ms,
        None
    );

    for invalid in [
        json!(0),
        json!(99),
        json!(3_600_001),
        json!(-1),
        json!(100.5),
        json!("30s"),
        json!(true),
    ] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "invalid-slow-start-upstream",
            "targets": [{"url": "https://example.com", "slowStartMs": invalid}]
        }]);
        let path = write_config(directory.path(), &config);
        load_and_compile_webserver_config(path)
            .expect_err("invalid slow-start value must fail closed");
    }

    let mut alias = base_config();
    alias["upstreams"] = json!([{
        "id": "invalid-slow-start-upstream",
        "targets": [{"url": "https://example.com", "slowStart": "30s"}]
    }]);
    let path = write_config(directory.path(), &alias);
    load_and_compile_webserver_config(path).expect_err("slow-start alias must fail closed");
}

#[test]
fn upstream_physical_connection_bounds_are_strict_and_coherent() {
    let directory = TempDir::new().expect("create temp directory");
    for invalid in [json!(0), json!(100_001)] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "bounded-upstream",
            "targets": [{"url": "https://example.com"}],
            "maxConnections": invalid
        }]);
        let path = write_config(directory.path(), &config);
        let error = load_and_compile_webserver_config(path)
            .expect_err("out-of-bound physical connection ceiling must fail");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.path.contains("maxConnections")));
    }

    let mut incoherent = base_config();
    incoherent["upstreams"] = json!([{
        "id": "bounded-upstream",
        "targets": [{"url": "https://example.com"}],
        "maxConnections": 8,
        "maxIdleConnections": 9
    }]);
    let path = write_config(directory.path(), &incoherent);
    let error = load_and_compile_webserver_config(path)
        .expect_err("idle connection ceiling above physical ceiling must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path.contains("maxIdleConnections")
            && diagnostic.message.contains("maxConnections")
    }));

    let mut unknown = base_config();
    unknown["upstreams"] = json!([{
        "id": "bounded-upstream",
        "targets": [{"url": "https://example.com"}],
        "connectionLimit": 8
    }]);
    let path = write_config(directory.path(), &unknown);
    let error = load_and_compile_webserver_config(path)
        .expect_err("unknown physical connection alias must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("connectionLimit")));
}

#[test]
fn target_physical_connection_bounds_are_strict_unambiguous_and_optional() {
    let directory = TempDir::new().expect("create temp directory");
    let mut bounded = base_config();
    bounded["upstreams"] = json!([{
        "id": "target-bounded-upstream",
        "targets": [
            {"url": "https://primary.example.com", "maxConnections": 1},
            {"url": "https://secondary.example.com", "maxConnections": 100_000},
            {"url": "https://default.example.com"}
        ],
        "maxConnections": 100_000
    }]);
    let path = write_config(directory.path(), &bounded);
    let compiled = load_and_compile_webserver_config(path)
        .expect("target physical connection boundaries must compile");
    assert_eq!(
        compiled.config().upstreams[0].targets[0].max_connections,
        Some(1)
    );
    assert_eq!(
        compiled.config().upstreams[0].targets[1].max_connections,
        Some(100_000)
    );
    assert_eq!(
        compiled.config().upstreams[0].targets[2].max_connections,
        None
    );

    for invalid in [json!(0), json!(100_001)] {
        let mut config = base_config();
        config["upstreams"] = json!([{
            "id": "target-bounded-upstream",
            "targets": [{"url": "https://example.com", "maxConnections": invalid}],
            "maxConnections": 100_000
        }]);
        let path = write_config(directory.path(), &config);
        let error = load_and_compile_webserver_config(path)
            .expect_err("out-of-range target physical connection ceiling must fail");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.path.contains("maxConnections")));
    }

    let mut above_upstream = base_config();
    above_upstream["upstreams"] = json!([{
        "id": "target-bounded-upstream",
        "targets": [{"url": "https://example.com", "maxConnections": 9}],
        "maxConnections": 8
    }]);
    let path = write_config(directory.path(), &above_upstream);
    let error = load_and_compile_webserver_config(path)
        .expect_err("target physical ceiling above upstream ceiling must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("cannot exceed upstream maxConnections")));

    let mut ambiguous = base_config();
    ambiguous["upstreams"] = json!([{
        "id": "target-bounded-upstream",
        "targets": [
            {"url": "https://example.com/primary", "maxConnections": 1},
            {"url": "https://EXAMPLE.com:443/secondary"}
        ],
        "maxConnections": 8
    }]);
    let path = write_config(directory.path(), &ambiguous);
    let error = load_and_compile_webserver_config(path)
        .expect_err("pool-ambiguous target authorities must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("authority must be unique")));

    let mut legacy = base_config();
    legacy["upstreams"] = json!([{
        "id": "legacy-upstream",
        "targets": [
            {"url": "https://example.com/primary"},
            {"url": "https://example.com/secondary"}
        ]
    }]);
    let path = write_config(directory.path(), &legacy);
    load_and_compile_webserver_config(path)
        .expect("omitted per-target limits preserve existing origin-pool behavior");
}

#[test]
fn active_upstream_health_contract_compiles_with_bounded_controls() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"maxConcurrentHealthChecks": 8});
    config["upstreams"] = json!([{
        "id": "actively-checked-upstream",
        "targets": [{"url": "https://example.com/base"}],
        "activeHealth": {
            "method": "HEAD",
            "uri": "/healthz?source=gateway",
            "intervalMs": 5_000,
            "timeoutMs": 1_000,
            "unhealthyThreshold": 4,
            "healthyThreshold": 3,
            "successStatusMin": 200,
            "successStatusMax": 299,
            "maxResponseBodyBytes": 0
        }
    }]);
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile active health policy");
    let health = compiled.config().upstreams[0]
        .active_health
        .as_ref()
        .expect("active health policy");
    assert_eq!(compiled.config().limits.max_concurrent_health_checks, 8);
    assert_eq!(health.uri, "/healthz?source=gateway");
    assert_eq!(health.interval_ms, 5_000);
    assert_eq!(health.timeout_ms, 1_000);
    assert_eq!(health.unhealthy_threshold, 4);
    assert_eq!(health.healthy_threshold, 3);
    assert_eq!(health.max_response_body_bytes, 0);
}

#[test]
fn active_upstream_health_rejects_unbounded_or_ambiguous_controls() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({"maxConcurrentHealthChecks": 1_025});
    config["upstreams"] = json!([{
        "id": "invalid-active-health",
        "targets": [{"url": "https://example.com"}],
        "activeHealth": {
            "method": "POST",
            "uri": "//attacker.example/health#fragment",
            "intervalMs": 99,
            "timeoutMs": 60_001,
            "unhealthyThreshold": 0,
            "healthyThreshold": 101,
            "successStatusMin": 600,
            "successStatusMax": 99,
            "maxResponseBodyBytes": 1_048_577
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("unbounded active health controls must fail");
    let diagnostics = error.diagnostics();
    for expected in [
        "maxConcurrentHealthChecks",
        "method",
        "intervalMs",
        "timeoutMs",
        "unhealthyThreshold",
        "healthyThreshold",
        "successStatusMin",
        "successStatusMax",
        "maxResponseBodyBytes",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.contains(expected)),
            "missing diagnostic for {expected}: {diagnostics:?}"
        );
    }
}

#[test]
fn active_upstream_health_rejects_authority_and_incoherent_ranges() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "ambiguous-active-health",
        "targets": [{"url": "https://example.com"}],
        "activeHealth": {
            "uri": "//attacker.example/health",
            "intervalMs": 1_000,
            "timeoutMs": 2_000,
            "successStatusMin": 300,
            "successStatusMax": 200
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("ambiguous active health policy must fail semantic validation");
    let diagnostics = error.diagnostics();
    for expected in ["uri", "timeoutMs", "successStatusMax"] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.contains(expected)),
            "missing diagnostic for {expected}: {diagnostics:?}"
        );
    }
}

#[test]
fn schema_rejects_unbounded_or_invalid_upstream_health_controls() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["upstreams"] = json!([{
        "id": "invalid-health-upstream",
        "targets": [{"url": "https://example.com"}],
        "maxInFlightRequests": 0,
        "passiveHealth": {
            "failureThreshold": 0,
            "ejectionTimeMs": 99,
            "failureStatuses": [499, 600]
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("invalid upstream health controls must fail schema validation");
    let diagnostics = error.diagnostics();
    for expected in [
        "maxInFlightRequests",
        "failureThreshold",
        "ejectionTimeMs",
        "failureStatuses",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.contains(expected)),
            "missing diagnostic for {expected}: {diagnostics:?}"
        );
    }
}

#[test]
fn schema_rejects_invalid_http2_keep_alive_policy() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "http2KeepAliveIntervalMs": 999,
        "http2KeepAliveTimeoutMs": 500
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("unsafe HTTP/2 Keep-Alive interval must fail Schema validation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("http2KeepAliveIntervalMs")));

    config["limits"] = json!({
        "http2KeepAliveIntervalMs": 1000,
        "http2KeepAliveTimeoutMs": 1001
    });
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(path)
        .expect_err("HTTP/2 Keep-Alive timeout beyond interval must fail semantic validation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("must not exceed its interval")));
}

#[test]
fn semantic_validation_rejects_incoherent_uri_query_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "maxUriPathBytes": 100,
        "maxDecodedPathBytes": 101,
        "maxQueryStringBytes": 0,
        "maxQueryParameters": 1,
        "maxQueryComponentBytes": 1
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("URI and Query budgets must remain coherent");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("maxDecodedPathBytes must not exceed maxUriPathBytes")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("query string, parameter, and component budgets")));
}

#[test]
fn semantic_validation_requires_canonical_route_paths() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["routes"][0]["match"]["path"] = json!("/api/../status");
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("non-canonical route path must not enter compiled indexes");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("route path must be canonical; use /status")));
}

#[test]
fn canonical_route_paths_allow_decoded_reserved_path_characters() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["routes"][0]["match"]["path"] = json!("/a?b#c%d");
    let path = write_config(directory.path(), &config);

    load_and_compile_webserver_config(path)
        .expect("reserved characters are data in a canonical Path, not URI delimiters");
}

#[test]
fn semantic_validation_rejects_unbounded_http2_per_connection_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "maxConnections": 1_000_000,
        "maxRequestHeaderBytes": 8_192,
        "http2MaxConcurrentStreams": 10_000,
        "http2MaxSendBufferBytes": 16_777_216,
        "http2MaxHeaderListBytes": 1_048_576
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("aggregate HTTP/2 connection budgets must remain bounded");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("send-buffer budget must not exceed 64 MiB")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("header-list budget must not exceed 64 MiB")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("connection header-window budget must not exceed 1 GiB")));
}

#[test]
fn semantic_validation_rejects_unbounded_http2_abuse_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "http2MaxConcurrentStreams": 10_000,
        "http2MaxSendBufferBytes": 1_024,
        "http2MaxHeaderListBytes": 1_024,
        "http2MaxFrameBytes": 16_384,
        "http2MaxFramesPerWindow": 100,
        "http2MaxNewStreamsPerWindow": 101,
        "http2MaxResetFramesPerWindow": 101,
        "http2MaxEncodedHeaderBlockBytes": 1_048_576
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("HTTP/2 abuse budgets must remain bounded");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("http2MaxNewStreamsPerWindow must not exceed http2MaxFramesPerWindow")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("http2MaxResetFramesPerWindow must not exceed http2MaxFramesPerWindow")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("encoded-header budget must not exceed 64 MiB")));
}

#[test]
fn semantic_validation_rejects_unbounded_process_request_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "maxConnections": 2_000,
        "maxConcurrentRequests": 2_000,
        "maxRequestHeaderBytes": 8_192,
        "http2MaxConcurrentStreams": 1,
        "http2MaxSendBufferBytes": 1_048_576,
        "http2MaxHeaderListBytes": 1_048_576,
        "http2MaxEncodedHeaderBlockBytes": 1_048_576
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("process request and connection budgets must remain bounded");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("active HTTP/2 header-list budget must not exceed 1 GiB")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("active HTTP/2 send-buffer budget must not exceed 1 GiB")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("encoded-header connection budget must not exceed 1 GiB")));
}

#[test]
fn semantic_validation_rejects_incoherent_http1_field_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "maxRequestHeaderBytes": 8_192,
        "maxRequestLineBytes": 8_192,
        "maxRequestMethodBytes": 32,
        "maxRequestTargetBytes": 8_193,
        "maxHeaderNameBytes": 256,
        "maxHeaderValueBytes": 8_193
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("individual HTTP/1 budgets must fit their enclosing budget");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("maxRequestTargetBytes must not exceed maxRequestLineBytes")));
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("maxHeaderValueBytes must not exceed maxRequestHeaderBytes")));
}

#[test]
fn semantic_validation_rejects_incoherent_trailer_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["limits"] = json!({
        "maxTrailerBytes": 0,
        "maxTrailers": 1
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path)
        .expect_err("trailer count cannot be enabled without a byte budget");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("must both be zero or both be positive")));
}

#[test]
fn validation_rejects_invalid_server_names() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["serverNames"][0] = json!("invalid host name");
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("invalid host name must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("serverNames")));
}

#[test]
fn semantic_validation_rejects_duplicate_ids_and_missing_references() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"][1]["id"] = json!("exact-response");
    config["virtualHosts"][0]["routes"][0]["resourceRef"] = json!("missing-resource");
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("invalid references must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate id")));
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown resource")));
}

#[test]
fn semantic_validation_rejects_static_root_escape() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "unsafe-static",
            "type": "static",
            "root": "../outside",
            "followSymlinks": false
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("unsafe path must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.ends_with("/root")));
}

#[test]
fn dynamic_tls_listener_allows_http2_and_rejects_static_policy_overlap() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["listeners"][0]["tlsRuntime"] = json!("assignment");
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile dynamic TLS listener");
    assert_eq!(
        compiled.config().listeners[0].tls_runtime,
        Some(sdkwork_webserver_core::ListenerTlsRuntime::Assignment)
    );

    config["listeners"][0]["tlsPolicyRef"] = json!("static-policy");
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(path)
        .expect_err("static and dynamic TLS authority must be exclusive");
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.path == "/listeners/0/tlsRuntime"
            && diagnostic.message.contains("cannot be combined")
    }));
}

#[test]
fn compilation_rejects_missing_tls_files() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["listeners"][0]["tlsPolicyRef"] = json!("public-tls");
    config["certificates"] = json!([{
        "id": "public-cert",
        "serverNames": ["example.com", "*.example.net"],
        "source": {
            "type": "protected-file",
            "certificateFile": "missing-cert.pem",
            "privateKeyFile": "missing-key.pem"
        }
    }]);
    config["tlsPolicies"] = json!([{
        "id": "public-tls",
        "certificateRef": "public-cert",
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3",
        "alpn": ["h2", "http/1.1"]
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("missing TLS files must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("certificateFile")));
}

#[test]
fn multiple_certificate_tls_policy_covers_all_listener_hosts() {
    let directory = TempDir::new().expect("create temp directory");
    for file in ["primary.pem", "primary.key", "wildcard.pem", "wildcard.key"] {
        fs::write(directory.path().join(file), b"test material").expect("write TLS material");
    }
    let mut config = base_config();
    config["listeners"][0]["tlsPolicyRef"] = json!("public-tls");
    config["certificates"] = json!([
        {
            "id": "primary-cert",
            "serverNames": ["example.com"],
            "source": {
                "type": "protected-file",
                "certificateFile": "primary.pem",
                "privateKeyFile": "primary.key"
            }
        },
        {
            "id": "wildcard-cert",
            "serverNames": ["*.example.net"],
            "source": {
                "type": "protected-file",
                "certificateFile": "wildcard.pem",
                "privateKeyFile": "wildcard.key"
            }
        }
    ]);
    config["tlsPolicies"] = json!([{
        "id": "public-tls",
        "certificateRefs": ["primary-cert", "wildcard-cert"],
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3",
        "alpn": ["http/1.1"]
    }]);

    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile multi-certificate TLS");
    let policy = compiled
        .tls_policy("public-tls")
        .expect("compiled TLS policy");
    assert_eq!(
        policy.certificate_refs().collect::<Vec<_>>(),
        ["primary-cert", "wildcard-cert"]
    );
}

#[test]
fn schema_rejects_ambiguous_tls_certificate_reference_shapes() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["tlsPolicies"] = json!([{
        "id": "invalid-tls",
        "certificateRef": "first",
        "certificateRefs": ["second"]
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("ambiguous shape must fail");
    assert!(matches!(error, WebServerConfigError::Validation { .. }));
}

#[test]
fn semantic_validation_rejects_normalized_certificate_name_duplicates() {
    let directory = TempDir::new().expect("create temp directory");
    fs::write(directory.path().join("cert.pem"), b"test").expect("write certificate");
    fs::write(directory.path().join("key.pem"), b"test").expect("write key");
    let mut config = base_config();
    config["certificates"] = json!([{
        "id": "duplicate-name-cert",
        "serverNames": ["EXAMPLE.com", "example.com."],
        "source": {
            "type": "protected-file",
            "certificateFile": "cert.pem",
            "privateKeyFile": "key.pem"
        }
    }]);
    let path = write_config(directory.path(), &config);

    let error =
        load_and_compile_webserver_config(path).expect_err("normalized duplicate must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("after DNS normalization")));
}

#[test]
fn semantic_validation_rejects_duplicate_sni_ownership_in_one_policy() {
    let directory = TempDir::new().expect("create temp directory");
    for file in ["first.pem", "first.key", "second.pem", "second.key"] {
        fs::write(directory.path().join(file), b"test").expect("write TLS material");
    }
    let mut config = base_config();
    config["listeners"][0]["tlsPolicyRef"] = json!("public-tls");
    config["certificates"] = json!([
        {
            "id": "first-cert",
            "serverNames": ["example.com", "*.example.net"],
            "source": {
                "type": "protected-file",
                "certificateFile": "first.pem",
                "privateKeyFile": "first.key"
            }
        },
        {
            "id": "second-cert",
            "serverNames": ["EXAMPLE.COM"],
            "source": {
                "type": "protected-file",
                "certificateFile": "second.pem",
                "privateKeyFile": "second.key"
            }
        }
    ]);
    config["tlsPolicies"] = json!([{
        "id": "public-tls",
        "certificateRefs": ["first-cert", "second-cert"],
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3",
        "alpn": ["http/1.1"]
    }]);
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("duplicate SNI owner must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("both declare server name example.com")));
}

#[test]
fn semantic_validation_rejects_listener_socket_conflicts() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"]
        .as_array_mut()
        .expect("listeners array")
        .push(json!({
            "id": "duplicate-socket",
            "bind": "127.0.0.1",
            "port": 18080,
            "protocols": ["http1"]
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("socket conflict must fail");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("already owns this bind and port")));
}

#[test]
fn loader_rejects_configuration_larger_than_one_megabyte() {
    let directory = TempDir::new().expect("create temp directory");
    let path = directory.path().join("oversized.json");
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).expect("write oversized config");

    let error = load_and_compile_webserver_config(path).expect_err("oversized config must fail");
    assert!(matches!(error, WebServerConfigError::TooLarge { .. }));
}

#[test]
fn acme_http01_webroot_resolves_and_is_confined() {
    let directory = TempDir::new().expect("create temp directory");
    fs::create_dir(directory.path().join("acme-webroot")).expect("create webroot");
    let mut config = base_config();
    config["listeners"][0]["acmeHttp01"] = json!({
        "webroot": "acme-webroot"
    });
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile ACME webroot");
    let listener = compiled.listener("http").expect("listener");
    assert!(listener.acme_http_01.is_some());
    let resolved = compiled.acme_webroot("http").expect("resolved webroot");
    assert_eq!(
        resolved,
        directory
            .path()
            .join("acme-webroot")
            .canonicalize()
            .expect("canonical")
    );
    assert!(resolved.starts_with(compiled.base_directory()));
    assert!(compiled.acme_webroot("unknown-listener").is_none());
    assert!(compiled.acme_webroot("http").is_some());
}

#[test]
fn acme_http01_webroot_escape_is_rejected() {
    let directory = TempDir::new().expect("create temp directory");
    let unique = format!(
        "outside-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let outside = directory.path().parent().expect("temp parent").join(unique);
    fs::create_dir(&outside).expect("create outside directory");
    let mut config = base_config();
    config["listeners"][0]["acmeHttp01"] = json!({
        "webroot": format!("../{}", outside.file_name().expect("name").to_string_lossy())
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("webroot escape must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("ACME webroot escapes")));
}

#[test]
fn acme_http01_webroot_unsafe_bytes_are_rejected() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["acmeHttp01"] = json!({
        "webroot": "acme\u{0}webroot"
    });
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("NUL webroot must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("NUL or control bytes")));
}

#[test]
fn drive_resource_compiles_and_is_listed_as_provider_resource() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "docs-drive",
            "type": "drive",
            "providerResourceUuid": "root-1234-5678",
            "resourceSubpath": "/site/docs",
            "indexFiles": ["index.html"],
            "spaFallback": "index.html",
            "cache": {
                "metadataTtlSeconds": 60,
                "negativeTtlSeconds": 10,
                "staleWhileRevalidateSeconds": 5
            }
        }));
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile drive resource");
    let resources: Vec<_> = compiled.provider_resources().collect();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].id(), "docs-drive");
    assert_eq!(
        resources[0].provider_type(),
        Some(ConfigProviderType::Drive)
    );
    assert_eq!(
        resources[0].provider_resource_uuid(),
        Some("root-1234-5678")
    );
    let cache = resources[0]
        .provider_cache_policy()
        .expect("cache policy present");
    assert_eq!(cache.metadata_ttl_seconds, 60);
    assert_eq!(cache.negative_ttl_seconds, 10);
    assert_eq!(cache.stale_while_revalidate_seconds, 5);
}

#[test]
fn knowledgebase_resource_compiles_and_is_listed_as_provider_resource() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "kb-docs",
            "type": "knowledgebase",
            "providerResourceUuid": "wiki-pub-1234",
            "locale": "zh-CN"
        }));
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile knowledgebase resource");
    let resources: Vec<_> = compiled.provider_resources().collect();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].id(), "kb-docs");
    assert_eq!(
        resources[0].provider_type(),
        Some(ConfigProviderType::Knowledgebase)
    );
    assert_eq!(resources[0].provider_resource_uuid(), Some("wiki-pub-1234"));
}

#[test]
fn provider_resources_omits_local_resources() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "docs-drive",
            "type": "drive",
            "providerResourceUuid": "root-1234"
        }));
    let path = write_config(directory.path(), &config);

    let compiled = load_and_compile_webserver_config(path).expect("compile mixed config");
    let resources: Vec<_> = compiled.provider_resources().collect();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].id(), "docs-drive");
    assert!(compiled.resource("exact-response").is_some());
}

#[test]
fn drive_resource_rejects_unsafe_subpath() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "unsafe-drive",
            "type": "drive",
            "providerResourceUuid": "root-1234",
            "resourceSubpath": "/../outside"
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("unsafe subpath must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.ends_with("/resourceSubpath")));
}

#[test]
fn drive_resource_rejects_non_index_index_files() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "indexed-drive",
            "type": "drive",
            "providerResourceUuid": "root-1234",
            "indexFiles": ["home.html"]
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("indexFiles must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.ends_with("/indexFiles")));
}

#[test]
fn drive_resource_rejects_blank_provider_resource_uuid() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "blank-drive",
            "type": "drive",
            "providerResourceUuid": "  "
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("blank uuid must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("/resources/")));
}

#[test]
fn knowledgebase_resource_rejects_invalid_locale() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "bad-locale-kb",
            "type": "knowledgebase",
            "providerResourceUuid": "wiki-pub-1234",
            "locale": "zh_CN"
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("invalid locale must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("/resources/")));
}

#[test]
fn knowledgebase_resource_rejects_index_files_at_schema_level() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["resources"]
        .as_array_mut()
        .expect("resources array")
        .push(json!({
            "id": "indexed-kb",
            "type": "knowledgebase",
            "providerResourceUuid": "wiki-pub-1234",
            "indexFiles": ["index.html"]
        }));
    let path = write_config(directory.path(), &config);

    let error = load_and_compile_webserver_config(path).expect_err("indexFiles must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.contains("/resources/")));
}

#[test]
fn public_listener_without_tls_is_rejected_unless_explicitly_allowed() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["bind"] = json!("0.0.0.0");
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(path).expect_err("public plaintext must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.ends_with("/bind")
            && diagnostic.message.contains("allowPlaintextHttp")));
}

#[test]
fn public_listener_plaintext_opt_in_and_acme_http01_are_accepted() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["listeners"][0]["bind"] = json!("0.0.0.0");
    config["listeners"][0]["allowPlaintextHttp"] = json!(true);
    let path = write_config(directory.path(), &config);
    load_and_compile_webserver_config(path).expect("explicit plaintext opt-in compiles");

    let mut config = base_config();
    config["listeners"][0]["bind"] = json!("0.0.0.0");
    config["listeners"][0]["acmeHttp01"] = json!({"webroot": "acme-webroot"});
    fs::create_dir(directory.path().join("acme-webroot")).expect("create webroot");
    let path = write_config(directory.path(), &config);
    load_and_compile_webserver_config(path).expect("acmeHttp01 plaintext compiles");
}

#[test]
fn security_headers_compile_and_hop_by_hop_custom_headers_are_rejected() {
    let directory = TempDir::new().expect("create temp directory");
    let mut config = base_config();
    config["virtualHosts"][0]["securityHeaders"] = json!({
        "strictTransportSecurity": {
            "maxAgeSeconds": 31536000,
            "includeSubDomains": true,
            "preload": true
        },
        "xFrameOptions": "DENY",
        "contentSecurityPolicy": "default-src 'self'",
        "referrerPolicy": "strict-origin-when-cross-origin",
        "xContentTypeOptions": true,
        "customHeaders": [
            {"name": "X-Custom-Tag", "value": "web-1"}
        ]
    });
    let path = write_config(directory.path(), &config);
    load_and_compile_webserver_config(path).expect("security headers compile");

    let mut config = base_config();
    config["virtualHosts"][0]["securityHeaders"] = json!({
        "customHeaders": [
            {"name": "Connection", "value": "close"}
        ]
    });
    let path = write_config(directory.path(), &config);
    let error = load_and_compile_webserver_config(path).expect_err("hop-by-hop header must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("hop-by-hop header")));
}
