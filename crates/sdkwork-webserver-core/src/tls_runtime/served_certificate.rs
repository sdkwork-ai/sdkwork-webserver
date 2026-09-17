//! The served-certificate report.
//!
//! The data plane is the only component that knows which certificate a server
//! name actually receives, because it is the component that resolves SNI. It
//! publishes that answer beside the TLS runtime snapshot the answer belongs to,
//! and the node daemon turns it into the `SERVED` certificate observations it
//! reports to the control plane.
//!
//! The report exists because a listener can serve two certificate sources at
//! once (see REQ-2026-0070). A per-hostname probe that asserts the assigned
//! fingerprint cannot express "the operator's own configured file legitimately
//! won this name", so it reports a failure for a listener that is serving
//! correctly. The resolution *and* the source it came from are the facts the
//! observation plane actually needs, and this document is those facts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Schema of the served-certificate report document.
///
/// Bumped only for a breaking shape change. A reader must refuse a report whose
/// version it does not know rather than interpreting whichever fields it happens
/// to recognise.
pub const SERVED_CERTIFICATE_REPORT_SCHEMA: &str = "sdkwork.tls-served-certificates/1";

/// Report file name, inside the TLS runtime snapshot's own directory.
pub const SERVED_CERTIFICATE_REPORT_FILE_NAME: &str = "tls-served-certificates.json";

/// Upper bound on a serialized report, for the producer and the reader alike.
///
/// One snapshot admits at most 256 assignments of at most 128 server names, so
/// this leaves room for the largest legitimate entry set while stopping a
/// corrupt or hostile resolver state from producing or consuming an unbounded
/// document.
pub const MAX_SERVED_CERTIFICATE_REPORT_BYTES: usize = 4 * 1024 * 1024;

/// Which certificate source answered a server name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CertificateSource {
    /// The operator's configured certificate files.
    Config,
    /// The certificate set assigned by the control plane.
    Assignment,
}

impl CertificateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Assignment => "assignment",
        }
    }
}

/// Path of the report belonging to the listener whose snapshot lives at
/// `snapshot_file`.
///
/// The report is a sibling of the snapshot rather than a separately configured
/// location, so a deployment that configures one path cannot end up with the
/// producer and a reader disagreeing about the other.
pub fn served_certificate_report_path(snapshot_file: &Path) -> PathBuf {
    match snapshot_file.parent() {
        Some(parent) => parent.join(SERVED_CERTIFICATE_REPORT_FILE_NAME),
        None => PathBuf::from(SERVED_CERTIFICATE_REPORT_FILE_NAME),
    }
}

/// What a listener serves for every server name its active snapshot assigns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedCertificateReport {
    /// [`SERVED_CERTIFICATE_REPORT_SCHEMA`] as written by the producer.
    pub schema_version: String,
    /// The listener the report describes.
    pub listener_id: String,
    /// The snapshot the report describes.
    ///
    /// Carried so a reader that knows which snapshot it expects can reject a
    /// report from a different rotation instead of acting on it.
    pub snapshot_sha256: String,
    /// Monotonic generation of that snapshot.
    pub generation: u64,
    /// Instant the producer resolved the entries, in the workspace default
    /// datetime format (`sdkwork_utils_rust::datetime`): UTC, millisecond
    /// precision.
    pub observed_at: String,
    pub entries: Vec<ServedCertificateEntry>,
}

/// What one assigned server name resolves to.
///
/// `served_fingerprint_sha256: None` means the handshake for this name fails, not
/// that the name was left unreported: `source` is `None` in exactly that case, so
/// the two fields can never disagree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedCertificateEntry {
    /// The node sync manifest's `certificateId`; the runtime snapshot calls the
    /// same value `certificateUuid`.
    pub certificate_id: String,
    pub server_name: String,
    /// Lowercase SHA-256 of the leaf this name receives.
    pub served_fingerprint_sha256: Option<String>,
    /// The source that provided that leaf.
    pub source: Option<CertificateSource>,
}
