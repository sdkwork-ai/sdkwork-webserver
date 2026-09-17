use sdkwork_webserver_core::tls_runtime::{
    compile_tls_assignment_snapshot, tls_assignment_snapshot_sha256, TlsAssignmentSnapshot,
    TlsRuntimeSnapshotError, MAX_TLS_RUNTIME_SNAPSHOT_BYTES,
};
use serde_json::{json, Value};

fn snapshot_fixture() -> Value {
    json!({
        "schemaVersion": "sdkwork.tls-runtime.v1",
        "kind": "sdkwork.tls-runtime.snapshot",
        "snapshotUuid": "tls-snapshot-0001",
        "nodeUuid": "web-node-0001",
        "generation": 1,
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-tls-compiler/1",
        "snapshotSha256": "0".repeat(64),
        "assignments": [
            {
                "assignmentUuid": "assignment-exact",
                "certificateUuid": "certificate-exact",
                "certificateVersion": "version-0002",
                "materialReference": "vault:certificate-exact-version-0002",
                "expectedFingerprintSha256": "2".repeat(64),
                "serverNames": ["api.example.com", "example.com"],
                "notBefore": "2026-07-20T00:00:00Z",
                "notAfter": "2026-10-18T00:00:00Z",
                "policy": {
                    "minimumVersion": "TLS1_2",
                    "maximumVersion": "TLS1_3",
                    "alpn": ["h2", "http/1.1"]
                }
            },
            {
                "assignmentUuid": "assignment-wildcard",
                "certificateUuid": "certificate-wildcard",
                "certificateVersion": "version-0007",
                "materialReference": "vault:certificate-wildcard-version-0007",
                "expectedFingerprintSha256": "7".repeat(64),
                "serverNames": ["*.example.com", "*.example.net"],
                "notBefore": "2026-07-20T00:00:00Z",
                "notAfter": "2026-10-18T00:00:00Z",
                "policy": {
                    "minimumVersion": "TLS1_3",
                    "maximumVersion": "TLS1_3",
                    "alpn": ["h2"]
                }
            }
        ],
        "limits": {
            "maximumAssignments": 128,
            "maximumServerNamesPerAssignment": 32
        }
    })
}

fn signed_snapshot(mut value: Value) -> Vec<u8> {
    let snapshot: TlsAssignmentSnapshot = serde_json::from_value(value.clone()).unwrap();
    value["snapshotSha256"] = Value::String(tls_assignment_snapshot_sha256(&snapshot).unwrap());
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn compiles_a_node_scoped_snapshot_and_reports_its_content_hash() {
    let compiled = compile_tls_assignment_snapshot(&signed_snapshot(snapshot_fixture()))
        .expect("fixture must compile");
    assert_eq!(compiled.snapshot().node_uuid, "web-node-0001");
    assert_eq!(compiled.snapshot().assignments.len(), 2);
    // The reported hash is the one the document carries: compiling re-derives
    // it from the canonical form and rejects the document when they disagree.
    assert_eq!(
        compiled.snapshot_sha256(),
        compiled.snapshot().snapshot_sha256
    );
}

#[test]
fn rejects_hash_tampering_and_private_key_material() {
    let error = compile_tls_assignment_snapshot(&serde_json::to_vec(&snapshot_fixture()).unwrap())
        .expect_err("unsigned fixture hash must fail");
    assert!(matches!(
        error,
        TlsRuntimeSnapshotError::HashMismatch { .. }
    ));

    let mut with_private_key: Value =
        serde_json::from_slice(&signed_snapshot(snapshot_fixture())).unwrap();
    with_private_key["assignments"][0]["privateKeyPem"] =
        Value::String("-----BEGIN PRIVATE KEY-----".into());
    let error = compile_tls_assignment_snapshot(&serde_json::to_vec(&with_private_key).unwrap())
        .expect_err("raw private key material must fail schema validation");
    assert!(matches!(error, TlsRuntimeSnapshotError::Validation { .. }));
}

#[test]
fn rejects_duplicate_sni_ownership_and_incoherent_tls_policy() {
    let mut duplicate = snapshot_fixture();
    duplicate["assignments"][1]["serverNames"] = json!(["api.example.com"]);
    let error = compile_tls_assignment_snapshot(&signed_snapshot(duplicate))
        .expect_err("duplicate SNI ownership must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("already assigned")));

    let mut policy = snapshot_fixture();
    policy["assignments"][0]["policy"]["minimumVersion"] = json!("TLS1_3");
    policy["assignments"][0]["policy"]["maximumVersion"] = json!("TLS1_2");
    let error = compile_tls_assignment_snapshot(&signed_snapshot(policy))
        .expect_err("incoherent TLS version policy must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path.ends_with("/policy/maximumVersion")));
}

#[test]
fn rejects_noncanonical_unicode_sni_but_exposes_the_canonical_idna_form() {
    let mut unicode = snapshot_fixture();
    unicode["assignments"][0]["serverNames"] = json!(["例子.测试"]);
    let error = compile_tls_assignment_snapshot(&signed_snapshot(unicode))
        .expect_err("producer must emit canonical ASCII IDNA names");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("xn--fsqu00a.xn--0zwm56d")));
}

#[test]
fn rejects_noncanonical_timestamps_and_oversized_snapshots() {
    let mut timestamp = snapshot_fixture();
    timestamp["generatedAt"] = json!("2026-07-21T00:00:00.000Z");
    let error = compile_tls_assignment_snapshot(&signed_snapshot(timestamp))
        .expect_err("producer timestamps must use one canonical representation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/generatedAt"));

    let oversized = vec![b' '; MAX_TLS_RUNTIME_SNAPSHOT_BYTES + 1];
    assert!(matches!(
        compile_tls_assignment_snapshot(&oversized),
        Err(TlsRuntimeSnapshotError::TooLarge { .. })
    ));
}

#[test]
fn rejects_zero_or_non_json_safe_generation() {
    for generation in [json!(0), json!(9_007_199_254_740_992_u64)] {
        let mut snapshot = snapshot_fixture();
        snapshot["generation"] = generation;
        let error = compile_tls_assignment_snapshot(&signed_snapshot(snapshot))
            .expect_err("generation must be a positive JSON-safe fencing token");
        assert!(error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.path == "/generation"));
    }
}

/// The snapshot is installed by the same code that indexes certificates for SNI,
/// so a name that normalizes as a website hostname but cannot be indexed has to
/// be refused here rather than when the assignment is installed.
#[test]
fn rejects_a_canonical_name_the_certificate_index_cannot_hold() {
    let mut underscore = snapshot_fixture();
    underscore["assignments"][0]["serverNames"] = json!(["_dmarc.example.com"]);
    let error = compile_tls_assignment_snapshot(&signed_snapshot(underscore))
        .expect_err("a name the data plane cannot index must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("the data plane can index")));

    let mut ip = snapshot_fixture();
    ip["assignments"][0]["serverNames"] = json!(["192.0.2.10"]);
    let error = compile_tls_assignment_snapshot(&signed_snapshot(ip))
        .expect_err("an IP literal cannot be a certificate server name");
    assert!(error.diagnostics().iter().any(|diagnostic| diagnostic
        .message
        .contains("must be an exact or leading-wildcard")));
}
