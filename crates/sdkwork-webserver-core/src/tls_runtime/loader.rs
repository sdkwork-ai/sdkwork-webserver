use std::sync::LazyLock;

use serde_json::Value;

use crate::json_schema::{compile_schema, schema_diagnostics};

use super::{
    tls_assignment_snapshot_sha256, validate_tls_assignment_snapshot,
    CompiledTlsAssignmentSnapshot, TlsAssignmentSnapshot, TlsRuntimeSnapshotError,
};

pub const MAX_TLS_RUNTIME_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
const SCHEMA: &str = include_str!("../../../../specs/sdkwork.tls-runtime.snapshot.schema.json");
static SCHEMA_VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| compile_schema(SCHEMA));

pub fn compile_tls_assignment_snapshot(
    bytes: &[u8],
) -> Result<CompiledTlsAssignmentSnapshot, TlsRuntimeSnapshotError> {
    if bytes.len() > MAX_TLS_RUNTIME_SNAPSHOT_BYTES {
        return Err(TlsRuntimeSnapshotError::TooLarge {
            actual_bytes: bytes.len(),
            maximum_bytes: MAX_TLS_RUNTIME_SNAPSHOT_BYTES,
        });
    }
    let instance: Value = serde_json::from_slice(bytes)?;
    validate_schema(&instance)?;
    let snapshot: TlsAssignmentSnapshot = serde_json::from_value(instance)?;
    let calculated = tls_assignment_snapshot_sha256(&snapshot)?;
    if snapshot.snapshot_sha256 != calculated {
        return Err(TlsRuntimeSnapshotError::HashMismatch {
            expected: snapshot.snapshot_sha256.clone(),
            calculated,
        });
    }
    validate_tls_assignment_snapshot(&snapshot)?;
    Ok(CompiledTlsAssignmentSnapshot::compile(snapshot, calculated))
}

fn validate_schema(instance: &Value) -> Result<(), TlsRuntimeSnapshotError> {
    let validator = SCHEMA_VALIDATOR
        .as_ref()
        .map_err(|error| TlsRuntimeSnapshotError::InvalidSchema(error.clone()))?;
    let diagnostics = schema_diagnostics(validator, instance);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(TlsRuntimeSnapshotError::Validation { diagnostics })
    }
}
