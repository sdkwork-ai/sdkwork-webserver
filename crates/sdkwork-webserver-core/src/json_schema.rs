//! Shared JSON Schema validation for the contract schemas embedded in this
//! crate.
//!
//! Every loader here validates a document against a schema it embeds with
//! `include_str!`. Compiling a schema costs about half a millisecond, while
//! validating one document against the compiled result costs a small fraction
//! of that, and the loaders that consume runtime snapshots run on a poll loop.
//! The compiled validator is therefore built once per process and reused.

use jsonschema::Validator;
use serde_json::Value;

use crate::ConfigDiagnostic;

/// Maximum number of schema violations reported for one document.
///
/// A document that violates the schema wholesale would otherwise produce one
/// entry per node, and these diagnostics travel in operator-facing payloads.
pub(crate) const MAX_SCHEMA_DIAGNOSTICS: usize = 64;

/// Maximum length of one diagnostic message, in bytes.
const MAX_DIAGNOSTIC_BYTES: usize = 512;

/// Compiles an embedded schema, or explains why it cannot be compiled.
///
/// Call through [`LazyLock::new`] so this runs at most once per process.
pub(crate) fn compile_schema(source: &'static str) -> Result<Validator, String> {
    let schema: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
    jsonschema::draft202012::new(&schema).map_err(|error| error.to_string())
}

/// The schema violations of `instance`, bounded to [`MAX_SCHEMA_DIAGNOSTICS`].
pub(crate) fn schema_diagnostics(validator: &Validator, instance: &Value) -> Vec<ConfigDiagnostic> {
    validator
        .iter_errors(instance)
        .take(MAX_SCHEMA_DIAGNOSTICS)
        .map(|error| {
            ConfigDiagnostic::new(
                error.instance_path().as_str(),
                truncate_diagnostic(&error.to_string()),
            )
        })
        .collect()
}

fn truncate_diagnostic(message: &str) -> String {
    if message.len() <= MAX_DIAGNOSTIC_BYTES {
        return message.to_owned();
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &message[..end])
}
