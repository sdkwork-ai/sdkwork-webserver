//! The verification-file vocabulary is one set, spelled in four places.
//!
//! A platform is renamed or added in the baseline DDL, in the service's
//! validator, in the contract the backend SDKs are generated from, and in the
//! console pickers. Every one of those is a place where a one-sided edit
//! compiles and ships: the console offers a platform the server refuses, or the
//! edge serves a file the schema cannot describe. This test reads the
//! machine-readable halves of that set and fails on either side of the
//! comparison.
//!
//! It reads only the authored baseline and the materialised contract, so it
//! needs no database and no network.

use std::collections::BTreeSet;

use sdkwork_intelligence_webserver_service::{
    VERIFICATION_FILE_EXTENSIONS, VERIFICATION_PLATFORMS,
};

const POSTGRES_BASELINE: &str =
    include_str!("../../../database/ddl/baseline/postgres/0001_webserver_baseline.sql");

/// The materialised backend-api contract, which is what the SDKs are generated
/// from.
const BACKEND_API_CONTRACT: &str =
    include_str!("../../../apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json");

const PLATFORM_CONSTRAINT: &str = "chk_webserver_domain_verification_file_platform";
const FILE_NAME_CONSTRAINT: &str = "chk_webserver_domain_verification_file_name";

/// The value list of `CONSTRAINT <name> CHECK (<column> IN ( … ))`.
fn admitted_values(baseline: &str, constraint: &str) -> BTreeSet<String> {
    let start = baseline
        .find(constraint)
        .unwrap_or_else(|| panic!("{constraint} is not declared in the baseline"));
    let rest = &baseline[start..];
    let open = rest
        .find("IN (")
        .unwrap_or_else(|| panic!("{constraint} carries no IN (…) list"))
        + "IN (".len();
    let close = rest[open..]
        .find(')')
        .unwrap_or_else(|| panic!("{constraint}'s IN (…) list is unterminated"));
    rest[open..open + close]
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn contract() -> serde_json::Value {
    serde_json::from_str(BACKEND_API_CONTRACT)
        .expect("the materialised backend-api contract is valid JSON")
}

fn contract_platforms(schema: &str) -> BTreeSet<String> {
    contract()["components"]["schemas"][schema]["properties"]["platform"]["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("{schema}.platform is not a closed enum"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{schema}.platform holds a non-string entry"))
                .to_owned()
        })
        .collect()
}

#[test]
fn the_ddl_and_the_service_agree_on_the_platform_set() {
    let ddl: BTreeSet<String> = admitted_values(POSTGRES_BASELINE, PLATFORM_CONSTRAINT);
    let service: BTreeSet<String> = VERIFICATION_PLATFORMS
        .iter()
        .map(|platform| (*platform).to_owned())
        .collect();
    assert_eq!(
        ddl, service,
        "the baseline's platform CHECK and VERIFICATION_PLATFORMS disagree; a stored value must be \
         one the service validates, and a validated value must be one the schema admits"
    );
}

#[test]
fn the_contract_admits_exactly_the_platforms_the_schema_does() {
    let ddl: BTreeSet<String> = admitted_values(POSTGRES_BASELINE, PLATFORM_CONSTRAINT);
    assert_eq!(
        contract_platforms("DomainVerificationFileResponse"),
        ddl,
        "DomainVerificationFileResponse.platform does not match the baseline"
    );
    assert_eq!(
        contract_platforms("CreateDomainVerificationFileRequest"),
        ddl,
        "CreateDomainVerificationFileRequest.platform does not match the baseline"
    );
}

#[test]
fn the_ddl_regex_is_the_contract_pattern_verbatim() {
    const EXPECTED_PATTERN: &str = r"'^[A-Za-z0-9][A-Za-z0-9._-]*\.(txt|html|xml)$'";
    let start = POSTGRES_BASELINE
        .find(FILE_NAME_CONSTRAINT)
        .expect("the file-name CHECK is declared");
    let region = &POSTGRES_BASELINE[start..];
    let region = &region[..region.find(';').expect("the CHECK statement terminates")];
    assert!(
        region.contains(EXPECTED_PATTERN),
        "{FILE_NAME_CONSTRAINT} no longer carries the expected pattern"
    );
    assert_eq!(
        contract()["components"]["schemas"]["CreateDomainVerificationFileRequest"]["properties"]
            ["fileName"]["pattern"]
            .as_str()
            .expect("the create request declares a pattern"),
        "^[A-Za-z0-9][A-Za-z0-9._-]*\\.(txt|html|xml)$",
        "the contract's fileName pattern drifted from the schema's"
    );
    let admitted: BTreeSet<String> = ["txt", "html", "xml"]
        .iter()
        .map(|extension| (*extension).to_owned())
        .collect();
    assert_eq!(
        VERIFICATION_FILE_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_owned())
            .collect::<BTreeSet<String>>(),
        admitted,
        "the service validates a different extension set than the schema and contract admit"
    );
}
