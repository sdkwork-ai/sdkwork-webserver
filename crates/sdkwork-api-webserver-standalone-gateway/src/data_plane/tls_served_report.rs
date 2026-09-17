//! Served-certificate report for the self-hosted TLS runtime.
//!
//! The component that resolves SNI names is the only one that knows which
//! certificate a name actually receives, so it publishes that answer as a local
//! artifact beside the TLS runtime snapshot. The node daemon reads the report
//! and derives its `SERVED` certificate observations from it.
//!
//! This exists because a listener may serve both a configured certificate file
//! and an assigned certificate (see REQ-2026-0070). An external probe that
//! asserts a single assigned fingerprint cannot express "the operator's own file
//! legitimately won this name", so it reports a false failure on a listener that
//! is serving correctly. The report states the resolution *and* its source, which
//! is the fact the observation plane actually needs.
//!
//! The document itself is defined by
//! [`sdkwork_webserver_core::tls_runtime`], next to the snapshot it describes and
//! shared with the reader, so the two processes cannot drift apart about its
//! shape. What lives here is the producer's half: building it from the layer set
//! that is actually installed, and publishing it so a reader sees one complete
//! revision at a time.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use sdkwork_webserver_core::tls_runtime::{
    ServedCertificateEntry, ServedCertificateReport, MAX_SERVED_CERTIFICATE_REPORT_BYTES,
    SERVED_CERTIFICATE_REPORT_SCHEMA,
};

use super::tls_resolver::ResolverLayers;

/// One certificate the active snapshot assigns, reduced to what the report
/// needs in order to describe it.
#[derive(Clone, Debug)]
pub(crate) struct AssignedCertificate {
    /// The snapshot's `certificateUuid`, which the node sync manifest calls
    /// `certificateId`; the report is keyed by the manifest's name so its one
    /// reader needs no translation.
    pub(crate) certificate_id: String,
    pub(crate) server_names: Vec<String>,
}

/// Describes what the listener will serve for every server name the given
/// snapshot assigns.
///
/// Entries are sorted and de-duplicated so identical resolution state always
/// serializes to identical bytes. A name cannot legitimately belong to two
/// assignments — the resolver rejects that when it builds its index — so
/// de-duplication only ever collapses an input that is already inconsistent.
pub(crate) fn build_served_certificate_report(
    listener_id: &str,
    snapshot_sha256: &str,
    generation: u64,
    observed_at: String,
    assigned: &[AssignedCertificate],
    layers: &ResolverLayers,
    now_unix_seconds: i64,
) -> ServedCertificateReport {
    let mut entries = Vec::new();
    for certificate in assigned {
        for server_name in &certificate.server_names {
            let resolved = layers.select_with_source(server_name, now_unix_seconds);
            entries.push(ServedCertificateEntry {
                certificate_id: certificate.certificate_id.clone(),
                server_name: server_name.clone(),
                served_fingerprint_sha256: resolved
                    .map(|(_, certificate)| certificate.fingerprint_sha256.clone()),
                source: resolved.map(|(source, _)| source),
            });
        }
    }
    entries.sort_by(|left, right| {
        left.server_name
            .cmp(&right.server_name)
            .then_with(|| left.certificate_id.cmp(&right.certificate_id))
    });
    entries.dedup_by(|left, right| {
        left.server_name == right.server_name && left.certificate_id == right.certificate_id
    });
    ServedCertificateReport {
        schema_version: SERVED_CERTIFICATE_REPORT_SCHEMA.to_owned(),
        listener_id: listener_id.to_owned(),
        snapshot_sha256: snapshot_sha256.to_owned(),
        generation,
        observed_at,
        entries,
    }
}

/// Publishes the report, replacing any previous revision atomically.
///
/// The write is staged beside the target and renamed into place, so a reader
/// either sees the previous complete report or the next one. That mirrors how the
/// snapshot itself is published, and matters because the daemon may read this
/// file at any instant.
pub(crate) fn write_served_certificate_report(
    path: &Path,
    report: &ServedCertificateReport,
) -> Result<(), String> {
    write_served_certificate_report_bounded(path, report, MAX_SERVED_CERTIFICATE_REPORT_BYTES)
}

fn write_served_certificate_report_bounded(
    path: &Path,
    report: &ServedCertificateReport,
    maximum_bytes: usize,
) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "served certificate report {} has no parent directory",
            path.display()
        )
    })?;
    let serialized = serde_json::to_vec(report)
        .map_err(|error| format!("serialize served certificate report: {error}"))?;
    if serialized.len() > maximum_bytes {
        return Err(format!(
            "served certificate report is {} bytes, above the {maximum_bytes} byte bound",
            serialized.len()
        ));
    }
    let staged = parent.join(staged_report_name());
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)
        .map_err(|error| {
            format!(
                "stage served certificate report {}: {error}",
                staged.display()
            )
        })?;
    file.write_all(&serialized)
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            format!(
                "write served certificate report {}: {error}",
                staged.display()
            )
        })?;
    drop(file);
    fs::rename(&staged, path).map_err(|error| {
        format!(
            "activate served certificate report {}: {error}",
            path.display()
        )
    })?;
    // Directory fsync makes the rename durable; a failure means the activated
    // report may not survive power loss and is reported instead of ignored.
    sync_directory(parent)
}

/// A staged name that cannot collide with a concurrent publisher or with a
/// leftover temporary file from a crashed one.
fn staged_report_name() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!(
        ".tls-served-certificates.tmp-{}-{nanos}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Makes the rename durable where the platform can express it.
///
/// Windows cannot open a directory as a file handle, so there the rename is left
/// to the filesystem's own metadata ordering. This is the same trade-off the TLS
/// recovery slots already make, and the alternative — refusing to publish on
/// Windows — would cost the observation plane its only authority.
#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), String> {
    let handle = fs::File::open(directory).map_err(|error| {
        format!(
            "open served certificate report directory {}: {error}",
            directory.display()
        )
    })?;
    handle.sync_all().map_err(|error| {
        format!(
            "sync served certificate report directory {}: {error}",
            directory.display()
        )
    })
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
// WORKSPACE-PATH:allow-fixture-block: this module is the file's #[cfg(test)] unit-test fixture data
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use rustls::crypto::aws_lc_rs;
    use rustls::pki_types::{pem::PemObject, PrivateKeyDer};
    use rustls::sign::CertifiedKey;
    use sdkwork_webserver_core::tls_runtime::{served_certificate_report_path, CertificateSource};
    use tempfile::TempDir;

    use super::super::tls_resolver::{CertificateSourceIndex, ResolvableCertificate};
    use super::*;

    const NOW: i64 = 1_700_000_000;
    const VALID_FROM: i64 = NOW - 1_000;
    const VALID_UNTIL: i64 = NOW + 1_000;

    fn certificate(fingerprint: &str) -> Arc<ResolvableCertificate> {
        let _ = aws_lc_rs::default_provider().install_default();
        let params =
            rcgen::CertificateParams::new(vec!["site.example.test".to_owned()]).expect("params");
        let key = rcgen::KeyPair::generate().expect("key pair");
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let private_key =
            PrivateKeyDer::from_pem_slice(key.serialize_pem().as_bytes()).expect("private key");
        let certified_key = CertifiedKey::from_der(
            vec![certificate.der().clone()],
            private_key,
            rustls::crypto::CryptoProvider::get_default().expect("installed provider"),
        )
        .expect("certified key");
        Arc::new(ResolvableCertificate::new(
            Arc::new(certified_key),
            fingerprint.to_owned(),
            VALID_FROM,
            VALID_UNTIL,
        ))
    }

    fn assigned(certificate_id: &str, names: &[&str]) -> AssignedCertificate {
        AssignedCertificate {
            certificate_id: certificate_id.to_owned(),
            server_names: names.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    fn layers(
        policy: &[(&str, Arc<ResolvableCertificate>)],
        assignment: &[(&str, Arc<ResolvableCertificate>)],
    ) -> ResolverLayers {
        let mut policy_index = CertificateSourceIndex::default();
        for (name, certificate) in policy {
            policy_index
                .insert(&[(*name).to_owned()], Arc::clone(certificate))
                .expect("unambiguous policy name");
        }
        let mut assignment_index = CertificateSourceIndex::default();
        for (name, certificate) in assignment {
            assignment_index
                .insert(&[(*name).to_owned()], Arc::clone(certificate))
                .expect("unambiguous assignment name");
        }
        ResolverLayers::new(policy_index, assignment_index)
    }

    #[test]
    fn report_path_is_a_sibling_of_the_snapshot() {
        assert_eq!(
            served_certificate_report_path(Path::new("/srv/tls/tls-runtime.json")),
            PathBuf::from("/srv/tls/tls-served-certificates.json")
        );
        assert_eq!(
            served_certificate_report_path(Path::new("tls-runtime.json")),
            PathBuf::from("tls-served-certificates.json")
        );
    }

    #[test]
    fn report_attributes_each_name_to_the_source_that_won() {
        let assigned_site = assigned("certificate-1", &["assigned.example.test"]);
        let shadowed_site = assigned(
            "certificate-2",
            &["shadowed.example.test", "uncovered.test"],
        );
        let report = build_served_certificate_report(
            "listener-a",
            "sha",
            7,
            "2026-09-17T00:00:00Z".to_owned(),
            &[assigned_site, shadowed_site],
            &layers(
                &[("shadowed.example.test", certificate(&"b".repeat(64)))],
                &[
                    ("assigned.example.test", certificate(&"c".repeat(64))),
                    ("shadowed.example.test", certificate(&"d".repeat(64))),
                ],
            ),
            NOW,
        );

        assert_eq!(report.schema_version, SERVED_CERTIFICATE_REPORT_SCHEMA);
        assert_eq!(report.generation, 7);
        // Sorted by server name, so the bytes are reproducible.
        let names = report
            .entries
            .iter()
            .map(|entry| entry.server_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "assigned.example.test",
                "shadowed.example.test",
                "uncovered.test"
            ]
        );

        let assigned_entry = &report.entries[0];
        assert_eq!(assigned_entry.certificate_id, "certificate-1");
        assert_eq!(assigned_entry.source, Some(CertificateSource::Assignment));
        assert_eq!(
            assigned_entry.served_fingerprint_sha256.as_deref(),
            Some("c".repeat(64).as_str())
        );

        // The configured file wins an assigned name: this is the case the old
        // probe could only report as a failure.
        let shadowed_entry = &report.entries[1];
        assert_eq!(shadowed_entry.certificate_id, "certificate-2");
        assert_eq!(shadowed_entry.source, Some(CertificateSource::Config));
        assert_eq!(
            shadowed_entry.served_fingerprint_sha256.as_deref(),
            Some("b".repeat(64).as_str()),
            "the configured file covers this name and is usable"
        );

        // Nothing covers this name, so the handshake fails there and the entry
        // must say so rather than implying a certificate.
        let uncovered_entry = &report.entries[2];
        assert!(uncovered_entry.served_fingerprint_sha256.is_none());
        assert!(uncovered_entry.source.is_none());
    }

    #[test]
    fn an_exact_name_outranks_a_wildcard_of_the_same_source() {
        // Both names are assigned certificates of their own; the exact claim
        // must win for itself and leave the wildcard to cover the rest.
        let exact = assigned("certificate-exact", &["exact.example.test"]);
        let wildcard = assigned("certificate-wildcard", &["*.example.test"]);
        let report = build_served_certificate_report(
            "listener-a",
            "sha",
            1,
            "2026-09-17T00:00:00Z".to_owned(),
            &[exact, wildcard],
            &layers(
                &[],
                &[
                    ("exact.example.test", certificate(&"1".repeat(64))),
                    ("*.example.test", certificate(&"2".repeat(64))),
                ],
            ),
            NOW,
        );

        let entry = report
            .entries
            .iter()
            .find(|entry| entry.server_name == "exact.example.test")
            .expect("the exact name has an entry");
        assert_eq!(entry.certificate_id, "certificate-exact");
        assert_eq!(
            entry.served_fingerprint_sha256.as_deref(),
            Some("1".repeat(64).as_str()),
            "an exact name outranks a wildcard in the same source"
        );
        // The wildcard assignment is still reported under its own literal name
        // rather than silently disappearing.
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.server_name == "*.example.test"
                && entry.certificate_id == "certificate-wildcard"));
    }

    #[test]
    fn publishing_replaces_the_previous_report_and_leaves_no_staging_file() {
        let root = TempDir::new().expect("tempdir");
        let path = root.path().join("tls-served-certificates.json");
        let build = |generation: u64| {
            build_served_certificate_report(
                "listener-a",
                "sha",
                generation,
                "2026-09-17T00:00:00Z".to_owned(),
                &[assigned("version-1", &["site.example.test"])],
                &layers(&[], &[("site.example.test", certificate(&"a".repeat(64)))]),
                NOW,
            )
        };

        write_served_certificate_report(&path, &build(1)).expect("first publish");
        write_served_certificate_report(&path, &build(2)).expect("second publish");

        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read report")).expect("parse report");
        assert_eq!(written["generation"], serde_json::json!(2));
        assert_eq!(
            written["entries"][0]["servedFingerprintSha256"],
            serde_json::json!("a".repeat(64))
        );

        let leftovers = fs::read_dir(root.path())
            .expect("read directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tls-served-certificates.tmp-")
            })
            .count();
        assert_eq!(leftovers, 0, "no staging file may survive a publish");
    }

    #[test]
    fn a_report_above_the_bound_is_rejected_without_touching_the_target() {
        let root = TempDir::new().expect("tempdir");
        let path = root.path().join("tls-served-certificates.json");
        fs::write(&path, b"previous").expect("seed previous report");
        let report = build_served_certificate_report(
            "listener-a",
            "sha",
            1,
            "2026-09-17T00:00:00Z".to_owned(),
            &[assigned("version-1", &["site.example.test"])],
            &layers(&[], &[("site.example.test", certificate(&"a".repeat(64)))]),
            NOW,
        );

        let error = write_served_certificate_report_bounded(&path, &report, 8)
            .expect_err("an oversized report must be rejected");
        assert!(error.contains("byte bound"), "unexpected error: {error}");
        assert_eq!(
            fs::read(&path).expect("read report"),
            b"previous",
            "a rejected report must not disturb the published one"
        );
    }
}
