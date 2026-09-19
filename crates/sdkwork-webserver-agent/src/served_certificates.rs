//! Deriving `SERVED` certificate observations from what the data plane reports.
//!
//! A node can serve a server name from either of two certificate sources: the
//! files its operator configured, or the certificate set the control plane
//! assigned (see REQ-2026-0070). Only the data plane knows which one won for a
//! given name, because it is the component that resolves SNI, so it publishes
//! [`ServedCertificateReport`] beside its TLS runtime snapshot and this module
//! turns that report into observations.
//!
//! Reading the data plane's own answer replaces a per-hostname probe for the
//! layered case. A probe asserts one fingerprint, so it cannot express "the
//! operator's configured file legitimately won this name" and reports a failure
//! for a listener that is serving correctly. What the observation plane needs is
//! the resolution together with the source that produced it — which is exactly
//! what the report records.
//!
//! Deployments that do not configure a TLS runtime keep the probe as their
//! authority; [`ServedCertificateAuthority`] selects between them from the
//! environment.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sdkwork_utils_rust::datetime;
use sdkwork_webserver_contract::{
    AgentCertificateBundle, AgentCertificateObservation, AgentSyncResponse,
};
use sdkwork_webserver_core::{
    runtime_env::TLS_RUNTIME_SNAPSHOT_FILE_ENV,
    tls_runtime::{
        served_certificate_report_path, CertificateSource, ServedCertificateReport,
        MAX_SERVED_CERTIFICATE_REPORT_BYTES, SERVED_CERTIFICATE_REPORT_SCHEMA,
    },
};
use tracing::{info, warn};

/// Observation state for a certificate the listener serves as intended.
const STATE_SERVED: &str = "SERVED";
/// Observation state for a certificate the listener does not serve.
const STATE_FAILED: &str = "FAILED";

/// The certificate's server names are not assigned to this node's data plane, so
/// nothing in the running listener can serve them.
const FAILURE_NAME_NOT_ASSIGNED: &str = "TLS_SERVER_NAME_NOT_ASSIGNED";
/// No certificate source covers the server name, so the handshake fails there.
const FAILURE_NAME_UNCOVERED: &str = "TLS_SERVER_NAME_UNCOVERED";
/// The name is served by a certificate other than the assigned one.
const FAILURE_FINGERPRINT_MISMATCH: &str = "TLS_SERVED_FINGERPRINT_MISMATCH";

/// Where the daemon learns what the listener actually serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServedCertificateAuthority {
    /// The data plane's served-certificate report, at this path.
    ///
    /// Derived from the TLS runtime snapshot file rather than configured
    /// separately: the report is published beside the snapshot, so naming the
    /// snapshot is enough for both processes to agree on where the handoff is
    /// and no deployment can point them at different files.
    Report(PathBuf),
    /// A per-hostname probe against the local listener.
    ///
    /// Correct for a listener that serves assigned certificates alone; a
    /// listener that layers configured files above the assigned set must
    /// configure the TLS runtime so its resolutions can be read instead.
    Probe,
}

impl ServedCertificateAuthority {
    pub(crate) fn from_env() -> Self {
        match std::env::var_os(TLS_RUNTIME_SNAPSHOT_FILE_ENV) {
            Some(snapshot_file) if !snapshot_file.is_empty() => {
                Self::Report(served_certificate_report_path(Path::new(&snapshot_file)))
            }
            _ => Self::Probe,
        }
    }

    /// The report path, when the daemon reads a report at all.
    pub(crate) fn report_path(&self) -> Option<&Path> {
        match self {
            Self::Report(path) => Some(path),
            Self::Probe => None,
        }
    }

    /// Announces which authority this node uses, and what that implies.
    ///
    /// Logged once at startup so an operator learns whether the probe's
    /// limitation applies to this node before it can mislead them. The probe
    /// cannot see a name the operator's configured files legitimately won, so a
    /// layered listener that runs on it reports false failures.
    pub(crate) fn log_selection(&self) {
        match self {
            Self::Report(path) => info!(
                served_certificate_report = %path.display(),
                "SERVED certificate observations are derived from the data plane's served-certificate report"
            ),
            Self::Probe => warn!(
                tls_runtime_snapshot_file = TLS_RUNTIME_SNAPSHOT_FILE_ENV,
                "SERVED certificate observations are derived from a per-hostname probe; a listener that layers configured certificate files above the assigned set must set this variable so its resolutions can be read"
            ),
        }
    }
}

/// Why a served-certificate report could not be used.
#[derive(Debug)]
pub(crate) enum ServedCertificateReportError {
    /// The data plane has not published a report yet.
    ///
    /// Expected while a node is starting up or between a rotation and the data
    /// plane adopting it, so the caller treats it as "nothing to observe yet"
    /// rather than as a fault.
    Absent,
    /// The report exists but cannot be trusted.
    Invalid(String),
}

/// A report that has been validated for size, shape, schema version, and
/// timestamp.
#[derive(Debug)]
pub(crate) struct ValidatedServedCertificateReport {
    report: ServedCertificateReport,
    observed_at: DateTime<Utc>,
}

impl ValidatedServedCertificateReport {
    pub(crate) fn report(&self) -> &ServedCertificateReport {
        &self.report
    }

    /// The instant the data plane resolved the report.
    ///
    /// A caller compares this against the instant it recorded its own
    /// deployment step to tell "the data plane has already reacted to this
    /// rotation" from "it has not looked yet".
    pub(crate) fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }
}

/// Reads and validates the served-certificate report at `path`.
pub(crate) fn read_served_certificate_report(
    path: &Path,
) -> Result<ValidatedServedCertificateReport, ServedCertificateReportError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ServedCertificateReportError::Absent);
        }
        Err(error) => {
            return Err(ServedCertificateReportError::Invalid(format!(
                "read metadata: {error}"
            )));
        }
    };
    // The report is published by rename, so a symlink or a directory where the
    // document should be means something other than the data plane wrote it.
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ServedCertificateReportError::Invalid(
            "not a regular file".to_owned(),
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_SERVED_CERTIFICATE_REPORT_BYTES as u64 {
        return Err(ServedCertificateReportError::Invalid(format!(
            "size {} is outside 1..={MAX_SERVED_CERTIFICATE_REPORT_BYTES} bytes",
            metadata.len()
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| ServedCertificateReportError::Invalid(format!("read: {error}")))?;
    if bytes.len() > MAX_SERVED_CERTIFICATE_REPORT_BYTES {
        return Err(ServedCertificateReportError::Invalid(format!(
            "size {} is above {MAX_SERVED_CERTIFICATE_REPORT_BYTES} bytes",
            bytes.len()
        )));
    }
    let report: ServedCertificateReport = serde_json::from_slice(&bytes)
        .map_err(|error| ServedCertificateReportError::Invalid(format!("decode: {error}")))?;
    if report.schema_version != SERVED_CERTIFICATE_REPORT_SCHEMA {
        return Err(ServedCertificateReportError::Invalid(format!(
            "unsupported schema version {}",
            report.schema_version
        )));
    }
    let observed_at = datetime::parse_datetime(&report.observed_at, None).ok_or_else(|| {
        ServedCertificateReportError::Invalid(format!(
            "observedAt {} is not a parseable datetime",
            report.observed_at
        ))
    })?;
    Ok(ValidatedServedCertificateReport {
        report,
        observed_at,
    })
}

/// One certificate's answer from the report.
enum ServedVerdict {
    Served,
    Failed(&'static str),
}

/// Rewrites the observed state of every certificate in `manifest` from what
/// `report` says the listener serves.
///
/// Returns `None` when every observation already states the verdict, so a steady
/// node neither rewrites its state file nor repeats a heartbeat. An observation
/// whose verdict cannot be established yet is left exactly as it was: a rotation
/// the data plane has not adopted must not read as a failure.
pub(crate) fn reconcile_served_certificate_observations(
    manifest: &AgentSyncResponse,
    report: &ValidatedServedCertificateReport,
    existing: &[AgentCertificateObservation],
    observed_at: &str,
) -> Option<Vec<AgentCertificateObservation>> {
    if manifest.certificates.is_empty() {
        return None;
    }
    let mut next = existing.to_vec();
    let mut changed = false;
    for certificate in &manifest.certificates {
        let recorded = next.iter().position(|observation| {
            observation.certificate_id == certificate.certificate_id
                && observation.fingerprint == certificate.fingerprint
                && observation.sync_version == manifest.sync_version
        });
        let (state, failure_code) = match certificate_verdict(certificate, report) {
            ServedVerdict::Served => (STATE_SERVED, None),
            ServedVerdict::Failed(code) => {
                // A failure may only be recorded once the data plane has
                // published a report newer than the moment this node recorded
                // its own step. Before that the report describes the previous
                // rotation, and calling that a failure would fail a deployment
                // that is merely still propagating.
                match recorded.map(|index| &next[index]) {
                    Some(observation) if report_is_after(report, &observation.observed_at) => {
                        (STATE_FAILED, Some(code))
                    }
                    _ => continue,
                }
            }
        };
        match recorded {
            Some(index) => {
                let observation = &mut next[index];
                if observation.state == state && observation.failure_code.as_deref() == failure_code
                {
                    continue;
                }
                observation.state = state.to_owned();
                observation.failure_code = failure_code.map(str::to_owned);
                observation.observed_at = observed_at.to_owned();
                changed = true;
            }
            None => {
                // No step was recorded for this generation in this process, so a
                // serving verdict is still worth reporting while an unconfirmed
                // failure is not: `continue` above already covers the latter.
                next.push(AgentCertificateObservation {
                    certificate_id: certificate.certificate_id.clone(),
                    fingerprint: certificate.fingerprint.clone(),
                    sync_version: manifest.sync_version.clone(),
                    state: state.to_owned(),
                    observed_at: observed_at.to_owned(),
                    failure_code: failure_code.map(str::to_owned),
                });
                changed = true;
            }
        }
    }
    changed.then_some(next)
}

/// Whether a certificate's names are all served as the control plane intended.
fn certificate_verdict(
    certificate: &AgentCertificateBundle,
    report: &ValidatedServedCertificateReport,
) -> ServedVerdict {
    let report = report.report();
    for server_name in &certificate.hostnames {
        let Some(entry) = report.entries.iter().find(|entry| {
            entry.certificate_id == certificate.certificate_id && entry.server_name == *server_name
        }) else {
            return ServedVerdict::Failed(FAILURE_NAME_NOT_ASSIGNED);
        };
        // A configured file that outranks the assigned certificate is the
        // operator's declared resolution, not a defect, so the name counts as
        // served. This is the case a fingerprint probe could only call a
        // failure.
        if entry.source == Some(CertificateSource::Config) {
            continue;
        }
        match entry.served_fingerprint_sha256.as_deref() {
            None => return ServedVerdict::Failed(FAILURE_NAME_UNCOVERED),
            Some(served) if served == certificate.fingerprint => {}
            Some(_) => return ServedVerdict::Failed(FAILURE_FINGERPRINT_MISMATCH),
        }
    }
    ServedVerdict::Served
}

/// Whether the report was resolved after `recorded_at`.
///
/// An unparseable instant is treated as "not after": the conservative answer is
/// to keep observing rather than to fail a certificate on a clock the daemon
/// cannot read.
fn report_is_after(report: &ValidatedServedCertificateReport, recorded_at: &str) -> bool {
    datetime::parse_datetime(recorded_at, None)
        .is_some_and(|recorded_at| datetime::is_after(report.observed_at(), recorded_at))
}

#[cfg(test)]
mod tests {
    use sdkwork_webserver_core::tls_runtime::{
        ServedCertificateEntry, SERVED_CERTIFICATE_REPORT_SCHEMA,
    };

    use super::*;

    const SYNC_VERSION: &str =
        "sv1:1111111111111111111111111111111111111111111111111111111111111111";
    const RECORDED_AT: &str = "2026-09-17T00:00:00.000Z";
    const ASSIGNED_FINGERPRINT: &str =
        "2222222222222222222222222222222222222222222222222222222222222222";
    const CONFIG_FINGERPRINT: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";

    fn manifest(hostnames: &[&str]) -> AgentSyncResponse {
        AgentSyncResponse {
            server_id: "server-0001".to_owned(),
            sync_version: SYNC_VERSION.to_owned(),
            unchanged: false,
            nginx_configs: Vec::new(),
            certificates: vec![AgentCertificateBundle {
                certificate_id: "certificate-0001".to_owned(),
                cert_name: "site.example.test".to_owned(),
                fingerprint: ASSIGNED_FINGERPRINT.to_owned(),
                hostnames: hostnames.iter().map(|name| (*name).to_owned()).collect(),
                fullchain_pem: String::new(),
                privkey_pem: String::new(),
            }],
        }
    }

    fn entry(
        server_name: &str,
        served: Option<&str>,
        source: Option<CertificateSource>,
    ) -> ServedCertificateEntry {
        ServedCertificateEntry {
            certificate_id: "certificate-0001".to_owned(),
            server_name: server_name.to_owned(),
            served_fingerprint_sha256: served.map(str::to_owned),
            source,
        }
    }

    fn report(entries: Vec<ServedCertificateEntry>) -> ValidatedServedCertificateReport {
        validated(entries, "2026-09-17T00:00:01.000Z")
    }

    fn validated(
        entries: Vec<ServedCertificateEntry>,
        observed_at: &str,
    ) -> ValidatedServedCertificateReport {
        ValidatedServedCertificateReport {
            report: ServedCertificateReport {
                schema_version: SERVED_CERTIFICATE_REPORT_SCHEMA.to_owned(),
                listener_id: "https-public".to_owned(),
                snapshot_sha256: "4".repeat(64),
                generation: 7,
                observed_at: observed_at.to_owned(),
                entries,
            },
            observed_at: datetime::parse_datetime(observed_at, None).expect("parse observed_at"),
        }
    }

    fn recorded(
        state: &str,
        failure_code: Option<&str>,
        observed_at: &str,
    ) -> Vec<AgentCertificateObservation> {
        vec![AgentCertificateObservation {
            certificate_id: "certificate-0001".to_owned(),
            fingerprint: ASSIGNED_FINGERPRINT.to_owned(),
            sync_version: SYNC_VERSION.to_owned(),
            state: state.to_owned(),
            observed_at: observed_at.to_owned(),
            failure_code: failure_code.map(str::to_owned),
        }]
    }

    fn reconcile(
        hostnames: &[&str],
        entries: Vec<ServedCertificateEntry>,
        existing: &[AgentCertificateObservation],
    ) -> Option<Vec<AgentCertificateObservation>> {
        reconcile_served_certificate_observations(
            &manifest(hostnames),
            &report(entries),
            existing,
            "2026-09-17T00:00:02.000Z",
        )
    }

    #[test]
    fn a_name_served_by_the_assigned_certificate_is_reported_served() {
        let observations = reconcile(
            &["site.example.test"],
            vec![entry(
                "site.example.test",
                Some(ASSIGNED_FINGERPRINT),
                Some(CertificateSource::Assignment),
            )],
            &recorded("ACTIVE", None, RECORDED_AT),
        )
        .expect("an active certificate becomes served");

        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].state, STATE_SERVED);
        assert_eq!(observations[0].failure_code, None);
        assert_eq!(observations[0].observed_at, "2026-09-17T00:00:02.000Z");
    }

    #[test]
    fn a_configured_file_that_outranks_the_assignment_is_not_a_failure() {
        // The data plane serves the operator's own certificate here because
        // `policy-first` says it may. The probe-based authority called this a
        // failure; the assignment is satisfied.
        let observations = reconcile(
            &["site.example.test"],
            vec![entry(
                "site.example.test",
                Some(CONFIG_FINGERPRINT),
                Some(CertificateSource::Config),
            )],
            &recorded("ACTIVE", None, RECORDED_AT),
        )
        .expect("a shadowed certificate still converges");

        assert_eq!(observations[0].state, STATE_SERVED);
        assert_eq!(observations[0].failure_code, None);
    }

    #[test]
    fn a_failure_is_recorded_once_the_data_plane_has_reacted() {
        for (name, served, source, expected) in [
            (
                "site.example.test",
                Some(CONFIG_FINGERPRINT),
                Some(CertificateSource::Assignment),
                FAILURE_FINGERPRINT_MISMATCH,
            ),
            ("site.example.test", None, None, FAILURE_NAME_UNCOVERED),
            ("other.example.test", None, None, FAILURE_NAME_NOT_ASSIGNED),
        ] {
            let observations = reconcile(
                &["site.example.test"],
                vec![entry(name, served, source)],
                &recorded("ACTIVE", None, RECORDED_AT),
            )
            .unwrap_or_else(|| panic!("{expected} must be recorded"));

            assert_eq!(observations[0].state, STATE_FAILED, "{expected}");
            assert_eq!(
                observations[0].failure_code.as_deref(),
                Some(expected),
                "{expected}"
            );
        }
    }

    #[test]
    fn a_report_older_than_the_deployment_is_left_unconfirmed() {
        // The data plane has not adopted this rotation yet, so the report still
        // describes the previous one. Nothing may be concluded from it.
        let existing = recorded("ACTIVE", None, RECORDED_AT);
        assert!(reconcile_served_certificate_observations(
            &manifest(&["site.example.test"]),
            &validated(
                vec![entry("site.example.test", None, None)],
                "2026-09-16T23:59:59.000Z"
            ),
            &existing,
            "2026-09-17T00:00:02.000Z",
        )
        .is_none());
    }

    #[test]
    fn a_recorded_verdict_is_not_rewritten() {
        let existing = recorded("SERVED", None, RECORDED_AT);
        assert!(reconcile(
            &["site.example.test"],
            vec![entry(
                "site.example.test",
                Some(ASSIGNED_FINGERPRINT),
                Some(CertificateSource::Assignment),
            )],
            &existing,
        )
        .is_none());
    }

    #[test]
    fn a_failed_certificate_recovers_when_the_data_plane_serves_it() {
        let observations = reconcile(
            &["site.example.test"],
            vec![entry(
                "site.example.test",
                Some(ASSIGNED_FINGERPRINT),
                Some(CertificateSource::Assignment),
            )],
            &recorded("FAILED", Some(FAILURE_FINGERPRINT_MISMATCH), RECORDED_AT),
        )
        .expect("a recovered certificate is reported");

        assert_eq!(observations[0].state, STATE_SERVED);
        assert_eq!(observations[0].failure_code, None);
    }

    #[test]
    fn a_served_report_is_read_from_disk_and_an_unknown_schema_is_refused() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("tls-served-certificates.json");
        assert!(matches!(
            read_served_certificate_report(&path),
            Err(ServedCertificateReportError::Absent)
        ));

        let written = report(vec![entry(
            "site.example.test",
            Some(ASSIGNED_FINGERPRINT),
            Some(CertificateSource::Assignment),
        )]);
        fs::write(&path, serde_json::to_vec(&written.report).expect("encode")).expect("write");
        let read = read_served_certificate_report(&path).expect("a valid report is accepted");
        assert_eq!(read.report().entries.len(), 1);
        assert_eq!(read.observed_at(), written.observed_at());

        let mut unsupported = written.report;
        unsupported.schema_version = "sdkwork.tls-served-certificates/2".to_owned();
        fs::write(&path, serde_json::to_vec(&unsupported).expect("encode")).expect("write");
        match read_served_certificate_report(&path).expect_err("version must be enforced") {
            ServedCertificateReportError::Invalid(reason) => {
                assert!(
                    reason.contains("schema version"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected an invalid report, got {other:?}"),
        }
    }

    #[test]
    fn the_authority_follows_the_shared_tls_runtime_setting() {
        // The report path is derived from the setting the data plane is given,
        // never configured a second time, so this is the only place the two
        // processes have to agree.
        let _guard = sdkwork_webserver_core::runtime_env::env_test_lock();
        let previous = std::env::var_os(TLS_RUNTIME_SNAPSHOT_FILE_ENV);
        std::env::remove_var(TLS_RUNTIME_SNAPSHOT_FILE_ENV);
        assert_eq!(
            ServedCertificateAuthority::from_env(),
            ServedCertificateAuthority::Probe
        );

        std::env::set_var(TLS_RUNTIME_SNAPSHOT_FILE_ENV, "/srv/tls/tls-runtime.json");
        assert_eq!(
            ServedCertificateAuthority::from_env(),
            ServedCertificateAuthority::Report(PathBuf::from(
                "/srv/tls/tls-served-certificates.json"
            ))
        );

        match previous {
            Some(value) => std::env::set_var(TLS_RUNTIME_SNAPSHOT_FILE_ENV, value),
            None => std::env::remove_var(TLS_RUNTIME_SNAPSHOT_FILE_ENV),
        }
    }
}
