//! DNS-01 challenge orchestration shared by every issuance path.
//!
//! RFC 8555 §8.4 derives the challenge record from the identifier: the label
//! `_acme-challenge` is prepended to the identifier with any leading `*.`
//! removed. A wildcard therefore shares one record name with its apex while
//! carrying a *different* TXT value, so presentations are keyed by
//! `(record_name, record_value)` and never replace each other. Publishing both
//! values at the same name is what lets a wildcard certificate be validated
//! regardless of the order the CA walks its authorizations.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use base64::Engine as _;

use crate::{AcmeServiceError, AcmeServiceResult};

/// The reserved ACME DNS-01 record label.
pub const ACME_CHALLENGE_LABEL: &str = "_acme-challenge";

/// Maximum labels in a DNS name, matching the deployments contract limit.
const MAX_HOSTNAME_BYTES: usize = 253;
const MAX_LABEL_BYTES: usize = 63;

/// Base64url (unpadded) encoding required by RFC 8555 §8.1 for TXT values.
const DNS01_VALUE_ENGINE: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Supported DNS provider families.
///
/// The wire values match the `deploy_dns_provider_credential.provider_kind`
/// baseline CHECK, so a row read from the control plane maps without a lookup
/// table of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DnsProviderKind {
    AliyunDns,
    Dnspod,
    Cloudflare,
}

impl DnsProviderKind {
    pub const ALL: [Self; 3] = [Self::AliyunDns, Self::Dnspod, Self::Cloudflare];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AliyunDns => "ALIYUN_DNS",
            Self::Dnspod => "DNSPOD",
            Self::Cloudflare => "CLOUDFLARE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ALIYUN_DNS" => Some(Self::AliyunDns),
            "DNSPOD" => Some(Self::Dnspod),
            "CLOUDFLARE" => Some(Self::Cloudflare),
            _ => None,
        }
    }
}

impl fmt::Display for DnsProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Normalizes an identifier and returns the DNS-01 record name it authorizes.
///
/// `example.com` and `*.example.com` both map to
/// `_acme-challenge.example.com`; any other `*` position is rejected because
/// ACME only ever authorizes a single leading label.
pub fn dns01_record_name(hostname: &str) -> AcmeServiceResult<String> {
    let base = normalized_identifier_base(hostname)?;
    Ok(format!("{ACME_CHALLENGE_LABEL}.{base}"))
}

/// Base64url-encodes a key-authorization digest into a TXT record value.
pub fn dns01_txt_value(key_authorization_digest: &[u8]) -> String {
    DNS01_VALUE_ENGINE.encode(key_authorization_digest)
}

/// Returns the record owner relative to a hosted zone apex.
///
/// Aliyun and DNSPod both address records as `(zone, relative owner)`, so the
/// absolute record name is reduced against the zone the operator configured
/// rather than against a guessed public suffix.
pub fn dns_relative_record_name(record_name: &str, zone_apex: &str) -> AcmeServiceResult<String> {
    let record = normalize_dns_name(record_name, "record name")?;
    let zone = normalize_dns_name(zone_apex, "zone apex")?;
    let relative = record
        .strip_suffix(&zone)
        .and_then(|prefix| prefix.strip_suffix('.'))
        .filter(|prefix| !prefix.is_empty());
    match relative {
        Some(relative) => Ok(relative.to_string()),
        None => Err(AcmeServiceError::validation(format!(
            "record {record} is not inside zone {zone}"
        ))),
    }
}

/// Validates and canonicalizes a DNS name for use in a challenge record.
pub fn normalize_dns_name(raw: &str, subject: &str) -> AcmeServiceResult<String> {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return Err(AcmeServiceError::validation(format!(
            "{subject} must not be empty"
        )));
    }
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.len() > MAX_HOSTNAME_BYTES {
        return Err(AcmeServiceError::validation(format!(
            "{subject} exceeds {MAX_HOSTNAME_BYTES} bytes"
        )));
    }
    if lowered.contains('*') {
        return Err(AcmeServiceError::validation(format!(
            "{subject} must not contain a wildcard label"
        )));
    }
    for label in lowered.split('.') {
        if label.is_empty() {
            return Err(AcmeServiceError::validation(format!(
                "{subject} contains an empty label"
            )));
        }
        if label.len() > MAX_LABEL_BYTES {
            return Err(AcmeServiceError::validation(format!(
                "{subject} contains a label longer than {MAX_LABEL_BYTES} bytes"
            )));
        }
        if !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(AcmeServiceError::validation(format!(
                "{subject} contains a character outside the ASCII DNS alphabet"
            )));
        }
    }
    if !lowered.contains('.') {
        return Err(AcmeServiceError::validation(format!(
            "{subject} must be a fully qualified domain name"
        )));
    }
    Ok(lowered)
}

/// One TXT value that must exist at `record_name` for a single authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dns01RecordRequest {
    /// Hosted zone apex that owns the record, for example `example.com`.
    pub zone_apex: String,
    /// Absolute record owner, for example `_acme-challenge.example.com`.
    pub record_name: String,
    /// TXT content: the base64url digest of the key authorization.
    pub record_value: String,
}

impl Dns01RecordRequest {
    pub fn new(
        zone_apex: impl Into<String>,
        record_name: impl Into<String>,
        record_value: impl Into<String>,
    ) -> AcmeServiceResult<Self> {
        let zone_apex = normalize_dns_name(&zone_apex.into(), "zone apex")?;
        let record_name = normalize_dns_name(&record_name.into(), "record name")?;
        let record_value = record_value.into();
        dns_relative_record_name(&record_name, &zone_apex)?;
        if record_value.is_empty() || record_value.len() > 512 {
            return Err(AcmeServiceError::validation(
                "DNS-01 record value must contain 1..512 bytes",
            ));
        }
        Ok(Self {
            zone_apex,
            record_name,
            record_value,
        })
    }

    /// The zone-relative owner used by Aliyun and DNSPod.
    pub fn relative_record_name(&self) -> AcmeServiceResult<String> {
        dns_relative_record_name(&self.record_name, &self.zone_apex)
    }
}

/// A published presentation, carrying whatever the provider needs to remove it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dns01RecordHandle {
    pub zone_apex: String,
    pub record_name: String,
    pub record_value: String,
    /// Provider-assigned record identity, when the provider returns one.
    pub provider_record_ref: Option<String>,
}

impl Dns01RecordHandle {
    pub fn request(&self) -> Dns01RecordRequest {
        Dns01RecordRequest {
            zone_apex: self.zone_apex.clone(),
            record_name: self.record_name.clone(),
            record_value: self.record_value.clone(),
        }
    }
}

impl From<&Dns01RecordRequest> for Dns01RecordHandle {
    fn from(request: &Dns01RecordRequest) -> Self {
        Self {
            zone_apex: request.zone_apex.clone(),
            record_name: request.record_name.clone(),
            record_value: request.record_value.clone(),
            provider_record_ref: None,
        }
    }
}

/// Presents and withdraws DNS-01 TXT records.
///
/// Implementations must be idempotent: [`Dns01Presenter::withdraw`] may be
/// called for a record that was already removed, and `publish` for a value
/// that already exists must not fail.
#[async_trait]
pub trait Dns01Presenter: Send + Sync {
    /// The provider family this presenter speaks to; `None` for a manual or
    /// in-memory presenter that has no external authority.
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        None
    }

    /// Publishes one TXT value. Concurrent presentations at the same record
    /// name must all survive; none may replace another.
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle>;

    /// Removes exactly the presentation described by `handle`.
    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()>;
}

/// Operator-facing presenter: it publishes nothing and relies on a human to
/// add the TXT record.
///
/// This is the fallback for zones whose DNS provider is not integrated. It is
/// deliberately not a no-op success: the caller is expected to surface the
/// request, wait for the operator, and only then let the CA validate.
#[derive(Debug, Default, Clone, Copy)]
pub struct ManualDns01Presenter;

#[async_trait]
impl Dns01Presenter for ManualDns01Presenter {
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        Ok(Dns01RecordHandle::from(request))
    }

    async fn withdraw(&self, _handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        Ok(())
    }
}

/// Deterministic presenter used by tests and dry runs.
///
/// Values accumulate per record name so the wildcard-plus-apex pair is
/// observable, which is exactly the invariant a provider adapter must not
/// break.
#[derive(Debug, Default)]
pub struct InMemoryDns01Presenter {
    records: Mutex<BTreeMap<String, Vec<String>>>,
}

impl InMemoryDns01Presenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the currently published TXT values for one record owner.
    pub fn values_at(&self, record_name: &str) -> Vec<String> {
        self.records
            .lock()
            .expect("dns01 store lock poisoned")
            .get(record_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Names currently carrying at least one value.
    pub fn record_names(&self) -> Vec<String> {
        self.records
            .lock()
            .expect("dns01 store lock poisoned")
            .keys()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl Dns01Presenter for InMemoryDns01Presenter {
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let mut guard = self
            .records
            .lock()
            .map_err(|_| AcmeServiceError::Internal("dns01 store lock poisoned".to_string()))?;
        let values = guard.entry(request.record_name.clone()).or_default();
        if !values.iter().any(|value| value == &request.record_value) {
            values.push(request.record_value.clone());
        }
        Ok(Dns01RecordHandle::from(request))
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        let mut guard = self
            .records
            .lock()
            .map_err(|_| AcmeServiceError::Internal("dns01 store lock poisoned".to_string()))?;
        if let Some(values) = guard.get_mut(&handle.record_name) {
            values.retain(|value| value != &handle.record_value);
            if values.is_empty() {
                guard.remove(&handle.record_name);
            }
        }
        Ok(())
    }
}

pub(crate) fn normalized_identifier_base(hostname: &str) -> AcmeServiceResult<String> {
    let trimmed = hostname.trim().trim_end_matches('.');
    let lowered = trimmed.to_ascii_lowercase();
    let base = lowered.strip_prefix("*.").unwrap_or(&lowered);
    if base.contains('*') {
        return Err(AcmeServiceError::validation(
            "only a single leading wildcard label is authorized",
        ));
    }
    normalize_dns_name(base, "DNS-01 identifier")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_apex_share_one_record_name() {
        assert_eq!(
            dns01_record_name("*.example.com").expect("wildcard"),
            "_acme-challenge.example.com"
        );
        assert_eq!(
            dns01_record_name("example.com").expect("apex"),
            "_acme-challenge.example.com"
        );
        assert_eq!(
            dns01_record_name("Example.COM.").expect("case and dot"),
            "_acme-challenge.example.com"
        );
    }

    #[test]
    fn rejects_non_leading_or_multi_level_wildcards() {
        assert!(dns01_record_name("a.*.example.com").is_err());
        assert!(dns01_record_name("*.*.example.com").is_err());
    }

    #[test]
    fn rejects_names_that_are_not_usable_records() {
        assert!(dns01_record_name("").is_err());
        assert!(dns01_record_name("localhost").is_err());
        assert!(dns01_record_name("a..example.com").is_err());
        assert!(dns01_record_name("space name.example.com").is_err());
        let long_label = "a".repeat(64);
        assert!(dns01_record_name(&format!("{long_label}.example.com")).is_err());
    }

    #[test]
    fn relative_name_is_reduced_against_the_zone() {
        assert_eq!(
            dns_relative_record_name("_acme-challenge.example.com", "example.com")
                .expect("apex zone"),
            "_acme-challenge"
        );
        assert_eq!(
            dns_relative_record_name("_acme-challenge.sub.example.com", "sub.example.com")
                .expect("sub zone"),
            "_acme-challenge"
        );
        assert_eq!(
            dns_relative_record_name("_acme-challenge.sub.example.com", "example.com")
                .expect("parent zone"),
            "_acme-challenge.sub"
        );
    }

    #[test]
    fn relative_name_rejects_a_record_outside_the_zone() {
        assert!(dns_relative_record_name("_acme-challenge.other.com", "example.com").is_err());
        assert!(dns_relative_record_name("example.com", "example.com").is_err());
    }

    #[test]
    fn txt_value_is_unpadded_base64url() {
        // RFC 8555 §8.1: base64url without padding, and no `+` or `/`.
        // `[0xff, 0xfe, 0xfd]` exercises `_` (index 63) and would emit `/` in
        // the standard alphabet, so it also pins url-safe encoding.
        let value = dns01_txt_value(&[0xff, 0xfe, 0xfd]);
        assert_eq!(value, "__79");
        assert!(!value.contains('='));
        assert!(!value.contains('+'));
        assert!(!value.contains('/'));
    }

    #[tokio::test]
    async fn wildcard_and_apex_values_coexist_at_one_name() {
        let presenter = InMemoryDns01Presenter::new();
        let apex = Dns01RecordRequest::new(
            "example.com",
            "_acme-challenge.example.com",
            dns01_txt_value(b"apex-auth"),
        )
        .expect("apex request");
        let wildcard = Dns01RecordRequest::new(
            "example.com",
            "_acme-challenge.example.com",
            dns01_txt_value(b"wildcard-auth"),
        )
        .expect("wildcard request");

        let apex_handle = presenter.publish(&apex).await.expect("publish apex");
        let wildcard_handle = presenter
            .publish(&wildcard)
            .await
            .expect("publish wildcard");

        let mut values = presenter.values_at("_acme-challenge.example.com");
        values.sort();
        assert_eq!(values.len(), 2, "both values must survive at one name");

        presenter
            .withdraw(&apex_handle)
            .await
            .expect("withdraw apex");
        assert_eq!(
            presenter.values_at("_acme-challenge.example.com"),
            vec![wildcard.record_value.clone()]
        );

        presenter
            .withdraw(&wildcard_handle)
            .await
            .expect("withdraw wildcard");
        assert!(presenter.record_names().is_empty());
    }

    #[tokio::test]
    async fn publishing_the_same_value_twice_is_idempotent() {
        let presenter = InMemoryDns01Presenter::new();
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "value-1")
                .expect("request");
        presenter.publish(&request).await.expect("first");
        presenter.publish(&request).await.expect("second");
        assert_eq!(presenter.values_at("_acme-challenge.example.com").len(), 1);
    }

    #[test]
    fn request_rejects_a_record_outside_its_zone() {
        assert!(
            Dns01RecordRequest::new("example.com", "_acme-challenge.other.com", "value").is_err()
        );
    }

    #[test]
    fn provider_kind_round_trips_through_the_baseline_wire_values() {
        for kind in DnsProviderKind::ALL {
            assert_eq!(DnsProviderKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DnsProviderKind::parse("ROUTE53"), None);
    }
}
