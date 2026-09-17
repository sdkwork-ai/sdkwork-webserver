use super::TlsAssignmentSnapshot;

/// A snapshot that passed schema, hash, and semantic validation.
///
/// Resolution is deliberately absent. SNI selection belongs to the data plane,
/// which ranks the certificates it has actually loaded; an index here could
/// only re-implement that precedence order from weaker inputs, and it would
/// have to keep agreeing with the resolver forever while nothing serves from it.
#[derive(Debug)]
pub struct CompiledTlsAssignmentSnapshot {
    snapshot: TlsAssignmentSnapshot,
    snapshot_sha256: String,
}

impl CompiledTlsAssignmentSnapshot {
    pub(crate) fn compile(snapshot: TlsAssignmentSnapshot, snapshot_sha256: String) -> Self {
        Self {
            snapshot,
            snapshot_sha256,
        }
    }

    pub fn snapshot(&self) -> &TlsAssignmentSnapshot {
        &self.snapshot
    }

    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }
}
