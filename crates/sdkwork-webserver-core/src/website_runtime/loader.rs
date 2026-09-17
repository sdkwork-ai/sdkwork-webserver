use std::sync::LazyLock;

use serde_json::Value;

use crate::json_schema::{compile_schema, schema_diagnostics};

use super::{
    validate_website_runtime_descriptor, website_runtime_descriptor_sha256,
    CompiledWebsiteRuntimeDescriptor, WebsiteRuntimeDescriptor, WebsiteRuntimeDescriptorError,
};

pub const MAX_WEBSITE_RUNTIME_DESCRIPTOR_BYTES: usize = 4 * 1024 * 1024;
const SCHEMA: &str =
    include_str!("../../../../specs/sdkwork.website-runtime.descriptor.schema.json");
static SCHEMA_VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| compile_schema(SCHEMA));

pub fn compile_website_runtime_descriptor(
    bytes: &[u8],
) -> Result<CompiledWebsiteRuntimeDescriptor, WebsiteRuntimeDescriptorError> {
    if bytes.len() > MAX_WEBSITE_RUNTIME_DESCRIPTOR_BYTES {
        return Err(WebsiteRuntimeDescriptorError::TooLarge {
            actual_bytes: bytes.len(),
            maximum_bytes: MAX_WEBSITE_RUNTIME_DESCRIPTOR_BYTES,
        });
    }
    let instance: Value = serde_json::from_slice(bytes)?;
    validate_schema(&instance)?;
    let descriptor: WebsiteRuntimeDescriptor = serde_json::from_value(instance)?;
    let calculated = website_runtime_descriptor_sha256(&descriptor)?;
    if descriptor.descriptor_sha256 != calculated {
        return Err(WebsiteRuntimeDescriptorError::HashMismatch {
            expected: descriptor.descriptor_sha256.clone(),
            calculated,
        });
    }
    validate_website_runtime_descriptor(&descriptor)?;
    Ok(CompiledWebsiteRuntimeDescriptor::compile(
        descriptor, calculated,
    ))
}

fn validate_schema(instance: &Value) -> Result<(), WebsiteRuntimeDescriptorError> {
    let validator = SCHEMA_VALIDATOR
        .as_ref()
        .map_err(|error| WebsiteRuntimeDescriptorError::InvalidSchema(error.clone()))?;
    let diagnostics = schema_diagnostics(validator, instance);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(WebsiteRuntimeDescriptorError::Validation { diagnostics })
    }
}
