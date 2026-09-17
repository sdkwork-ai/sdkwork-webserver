use std::collections::HashSet;

use crate::{
    normalize_tls_server_name, website_runtime::normalize_website_hostname, ConfigDiagnostic,
};

use super::{
    TlsAssignmentSnapshot, TlsRuntimeSnapshotError, TLS_RUNTIME_SCHEMA_VERSION,
    TLS_RUNTIME_SNAPSHOT_KIND,
};

const MAX_DIAGNOSTICS: usize = 128;
const HARD_MAX_ASSIGNMENTS: usize = 10_000;
const HARD_MAX_SERVER_NAMES_PER_ASSIGNMENT: usize = 100;
const HARD_MAX_GENERATION: u64 = 9_007_199_254_740_991;

pub(crate) fn validate_tls_assignment_snapshot(
    snapshot: &TlsAssignmentSnapshot,
) -> Result<(), TlsRuntimeSnapshotError> {
    let mut validator = TlsRuntimeValidator::default();
    validator.validate(snapshot);
    validator.finish()
}

#[derive(Default)]
struct TlsRuntimeValidator {
    diagnostics: Vec<ConfigDiagnostic>,
}

impl TlsRuntimeValidator {
    fn validate(&mut self, snapshot: &TlsAssignmentSnapshot) {
        if snapshot.schema_version != TLS_RUNTIME_SCHEMA_VERSION {
            self.push(
                "/schemaVersion",
                format!("only {TLS_RUNTIME_SCHEMA_VERSION} is supported"),
            );
        }
        if snapshot.kind != TLS_RUNTIME_SNAPSHOT_KIND {
            self.push("/kind", format!("kind must be {TLS_RUNTIME_SNAPSHOT_KIND}"));
        }
        self.validate_id("/snapshotUuid", &snapshot.snapshot_uuid);
        self.validate_id("/nodeUuid", &snapshot.node_uuid);
        if snapshot.generation == 0 || snapshot.generation > HARD_MAX_GENERATION {
            self.push(
                "/generation",
                format!("must be between 1 and JSON-safe ceiling {HARD_MAX_GENERATION}"),
            );
        }
        self.validate_timestamp("/generatedAt", &snapshot.generated_at);
        if snapshot.compiler_version.is_empty()
            || snapshot.compiler_version.len() > 128
            || snapshot
                .compiler_version
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            self.push(
                "/compilerVersion",
                "must be a non-empty bounded version token",
            );
        }
        if !is_lower_hex(&snapshot.snapshot_sha256, 64) {
            self.push(
                "/snapshotSha256",
                "must contain exactly 64 lowercase hexadecimal characters",
            );
        }
        self.validate_limits(snapshot);
        self.validate_assignments(snapshot);
    }

    fn validate_limits(&mut self, snapshot: &TlsAssignmentSnapshot) {
        let limits = &snapshot.limits;
        if limits.maximum_assignments == 0 || limits.maximum_assignments > HARD_MAX_ASSIGNMENTS {
            self.push(
                "/limits/maximumAssignments",
                format!("must be between 1 and runtime ceiling {HARD_MAX_ASSIGNMENTS}"),
            );
        }
        if limits.maximum_server_names_per_assignment == 0
            || limits.maximum_server_names_per_assignment > HARD_MAX_SERVER_NAMES_PER_ASSIGNMENT
        {
            self.push(
                "/limits/maximumServerNamesPerAssignment",
                format!(
                    "must be between 1 and runtime ceiling {HARD_MAX_SERVER_NAMES_PER_ASSIGNMENT}"
                ),
            );
        }
        if snapshot.assignments.len() > limits.maximum_assignments {
            self.push(
                "/assignments",
                "assignment count exceeds maximumAssignments",
            );
        }
    }

    fn validate_assignments(&mut self, snapshot: &TlsAssignmentSnapshot) {
        if snapshot
            .assignments
            .windows(2)
            .any(|pair| pair[0].assignment_uuid > pair[1].assignment_uuid)
        {
            self.push("/assignments", "entries must be ordered by assignmentUuid");
        }
        let mut assignment_ids = HashSet::new();
        let mut server_name_owners = HashSet::new();
        for (index, assignment) in snapshot.assignments.iter().enumerate() {
            let path = format!("/assignments/{index}");
            self.validate_id(
                &format!("{path}/assignmentUuid"),
                &assignment.assignment_uuid,
            );
            if !assignment_ids.insert(assignment.assignment_uuid.as_str()) {
                self.push(
                    format!("{path}/assignmentUuid"),
                    "assignmentUuid is duplicated",
                );
            }
            self.validate_id(
                &format!("{path}/certificateUuid"),
                &assignment.certificate_uuid,
            );
            self.validate_id(
                &format!("{path}/certificateVersion"),
                &assignment.certificate_version,
            );
            if !valid_material_reference(&assignment.material_reference) {
                self.push(
                    format!("{path}/materialReference"),
                    "must be a bounded opaque secret-provider reference",
                );
            }
            if !is_lower_hex(&assignment.expected_fingerprint_sha256, 64) {
                self.push(
                    format!("{path}/expectedFingerprintSha256"),
                    "must contain exactly 64 lowercase hexadecimal characters",
                );
            }
            self.validate_timestamp(&format!("{path}/notBefore"), &assignment.not_before);
            self.validate_timestamp(&format!("{path}/notAfter"), &assignment.not_after);
            if assignment.not_before >= assignment.not_after {
                self.push(format!("{path}/notAfter"), "must be later than notBefore");
            }
            if assignment.server_names.is_empty() {
                self.push(
                    format!("{path}/serverNames"),
                    "at least one server name is required",
                );
            }
            if assignment.server_names.len() > snapshot.limits.maximum_server_names_per_assignment {
                self.push(
                    format!("{path}/serverNames"),
                    "server name count exceeds maximumServerNamesPerAssignment",
                );
            }
            if assignment
                .server_names
                .windows(2)
                .any(|pair| pair[0] > pair[1])
            {
                self.push(
                    format!("{path}/serverNames"),
                    "server names must be in canonical order",
                );
            }
            for (name_index, server_name) in assignment.server_names.iter().enumerate() {
                // The website normalizer is consulted for its IDNA conversion:
                // it is what names the canonical ASCII form a control plane has
                // to send for a Unicode name. What the data plane can index is
                // narrower than that, so a canonical name it cannot index - an
                // underscore label, say - is refused here instead of failing the
                // install of an assignment that passed validation.
                match normalize_website_hostname(server_name) {
                    Some(normalized) if normalized != *server_name => self.push(
                        format!("{path}/serverNames/{name_index}"),
                        format!("must use canonical lowercase ASCII server name {normalized}"),
                    ),
                    Some(_) if normalize_tls_server_name(server_name).is_none() => self.push(
                        format!("{path}/serverNames/{name_index}"),
                        "must be an exact or leading-wildcard DNS server name the data plane can index",
                    ),
                    Some(_) => {}
                    None => self.push(
                        format!("{path}/serverNames/{name_index}"),
                        "must be an exact or leading-wildcard DNS server name",
                    ),
                }
                if !server_name_owners.insert(server_name.as_str()) {
                    self.push(
                        format!("{path}/serverNames/{name_index}"),
                        "server name is already assigned to another certificate",
                    );
                }
            }
            if assignment.policy.minimum_version > assignment.policy.maximum_version {
                self.push(
                    format!("{path}/policy/maximumVersion"),
                    "must not be lower than minimumVersion",
                );
            }
            if assignment.policy.alpn.is_empty()
                || assignment.policy.alpn.len() > 2
                || assignment
                    .policy
                    .alpn
                    .iter()
                    .any(|value| value != "h2" && value != "http/1.1")
            {
                self.push(
                    format!("{path}/policy/alpn"),
                    "must contain a bounded subset of h2 and http/1.1",
                );
            }
            let mut alpn = HashSet::new();
            if assignment
                .policy
                .alpn
                .iter()
                .any(|value| !alpn.insert(value.as_str()))
            {
                self.push(format!("{path}/policy/alpn"), "ALPN value is duplicated");
            }
        }
    }

    fn validate_id(&mut self, path: &str, value: &str) {
        if !valid_opaque_id(value) {
            self.push(path, "must be a bounded opaque identifier");
        }
    }

    fn validate_timestamp(&mut self, path: &str, value: &str) {
        if !valid_canonical_timestamp(value) {
            self.push(path, "must use canonical UTC RFC 3339 seconds format");
        }
    }

    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(ConfigDiagnostic::new(path, message));
        }
    }

    fn finish(self) -> Result<(), TlsRuntimeSnapshotError> {
        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(TlsRuntimeSnapshotError::Validation {
                diagnostics: self.diagnostics,
            })
        }
    }
}

fn valid_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_material_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.contains(':')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_canonical_timestamp(value: &str) -> bool {
    value.len() == 20
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value.as_bytes()[10] == b'T'
        && value.as_bytes()[13] == b':'
        && value.as_bytes()[16] == b':'
        && value.as_bytes()[19] == b'Z'
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        })
}
