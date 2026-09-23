//! Cloud-account ↔ domain association for DNS-01 challenges
//! (PRD: 关联云账号实现完整的自动续签能力).
//!
//! A [`DnsCloudAccount`] binds one hosted zone (`example.com`) on one DNS
//! provider (Aliyun / DNSPod / Cloudflare) to that account's credentials,
//! materialized as a [`Dns01Presenter`]. The [`DnsCloudAccountRegistry`]
//! resolves any certificate identifier to its covering zone by longest
//! suffix (so `www.example.com`, `example.com`, and `*.example.com` all
//! resolve to the `example.com` account; a deeper `dev.example.com` zone
//! wins over the apex), and dispatches TXT publications to the owning
//! account.
//!
//! With a registry attached, certificate issuance **and renewal** use
//! DNS-01 — wildcards renew unattended, and no operator ever hand-edits a
//! TXT record. Without one, issuance falls back to HTTP-01 (wildcards are
//! rejected up front) and the manual DNS-01 presenter remains the operator
//! fallback.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::dns::{
    normalize_dns_name, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest,
    DnsAccountVerification, DnsProviderKind,
};
use crate::{AcmeServiceError, AcmeServiceResult};

/// Wire/JSON shape of one associated cloud account (from configuration or
/// the control plane): provider family, hosted zone, credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DnsCloudAccountConfig {
    /// Stable operator-facing reference.
    #[serde(rename = "accountId")]
    pub account_id: String,
    /// Provider family: `ALIYUN_DNS`, `DNSPOD`, `CLOUDFLARE`, `HTTP_REQUEST`.
    pub provider: String,
    /// Hosted zone apex the account owns, e.g. `example.com`.
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,
    /// Provider credentials. Aliyun/DNSPod: `{ accessKeyId, accessKeySecret }`;
    /// Cloudflare: `{ apiToken }`; `HTTP_REQUEST`: the request configuration
    /// document itself (see `dns_http_request`), because that family treats its
    /// requests as data rather than as a fixed credential shape.
    pub credentials: serde_json::Value,
}

/// One associated cloud DNS account: a hosted zone plus the provider
/// presenter built from that account's credentials.
#[derive(Clone)]
pub struct DnsCloudAccount {
    /// Stable operator-facing reference (audit / diagnostics).
    pub account_id: String,
    /// Hosted zone apex the account owns, e.g. `example.com`.
    pub zone_apex: String,
    /// Provider family.
    pub provider: DnsProviderKind,
    /// Presenter constructed from the account credentials.
    pub presenter: Arc<dyn Dns01Presenter>,
}

impl std::fmt::Debug for DnsCloudAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DnsCloudAccount")
            .field("account_id", &self.account_id)
            .field("zone_apex", &self.zone_apex)
            .field("provider", &self.provider.as_str())
            .finish_non_exhaustive()
    }
}

impl DnsCloudAccount {
    /// True when this account's zone covers `hostname` (the hostname equals
    /// the zone or lives beneath it). Wildcard identifiers are normalized to
    /// their base before matching.
    pub fn covers(&self, hostname: &str) -> bool {
        match crate::dns::normalized_identifier_base(hostname) {
            Ok(base) => base == self.zone_apex || base.ends_with(&format!(".{}", self.zone_apex)),
            Err(_) => false,
        }
    }
}

/// Association registry: resolves identifiers to covering accounts with
/// longest-zone-suffix precedence and dispatches DNS-01 presentations to the
/// owning account.
#[derive(Default)]
pub struct DnsCloudAccountRegistry {
    accounts: Vec<DnsCloudAccount>,
}

impl DnsCloudAccountRegistry {
    /// An empty registry (DNS-01 unavailable; HTTP-01-only renewal).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Registers an account. Duplicate zone apices are rejected: one zone,
    /// one authoritative account.
    pub fn register(&mut self, account: DnsCloudAccount) -> Result<(), AcmeServiceError> {
        let zone = normalize_dns_name(&account.zone_apex, "zone apex")?;
        if self
            .accounts
            .iter()
            .any(|existing| existing.zone_apex == zone)
        {
            return Err(AcmeServiceError::validation(format!(
                "zone {zone} is already associated with another cloud account"
            )));
        }
        self.accounts.push(DnsCloudAccount {
            zone_apex: zone,
            ..account
        });
        Ok(())
    }

    /// Number of associated accounts.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Every associated zone apex, in registration order.
    ///
    /// For startup diagnostics: "which zones can renew unattended" must be
    /// answerable from a log line, and it never exposes the account id or any
    /// credential.
    pub fn zone_apices(&self) -> Vec<String> {
        self.accounts
            .iter()
            .map(|account| account.zone_apex.clone())
            .collect()
    }

    /// True when no cloud account is associated.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Resolves the covering account for one identifier by longest
    /// zone-suffix match.
    pub fn resolve(&self, hostname: &str) -> Option<&DnsCloudAccount> {
        let mut best: Option<(&DnsCloudAccount, usize)> = None;
        for account in &self.accounts {
            if !account.covers(hostname) {
                continue;
            }
            let depth = account.zone_apex.split('.').count();
            if best.is_none_or(|(_, best_depth)| depth > best_depth) {
                best = Some((account, depth));
            }
        }
        best.map(|(account, _)| account)
    }

    /// Builds the registry from account configs, constructing each
    /// provider presenter from its credentials. A credential failure for
    /// one account fails the registry build (fail closed: a renewal that
    /// silently skips an account would strand its certificates).
    pub fn from_configs(configs: &[DnsCloudAccountConfig]) -> AcmeServiceResult<Self> {
        let mut registry = Self::empty();
        for config in configs {
            let provider = DnsProviderKind::parse(&config.provider).ok_or_else(|| {
                AcmeServiceError::validation(format!("unknown DNS provider `{}`", config.provider))
            })?;
            let presenter: Arc<dyn Dns01Presenter> = match (&provider, &config.credentials) {
                (DnsProviderKind::AliyunDns, serde_json::Value::Object(credentials)) => {
                    let client = crate::dns_http::DnsApiClient::new()?;
                    let access_key_id = credential_string(credentials, "accessKeyId")?;
                    let access_key_secret = credential_string(credentials, "accessKeySecret")?;
                    Arc::new(crate::dns_aliyun::AliyunDns01Presenter::new(
                        client,
                        access_key_id,
                        access_key_secret,
                    )?)
                }
                (DnsProviderKind::Dnspod, serde_json::Value::Object(credentials)) => {
                    let client = crate::dns_http::DnsApiClient::new()?;
                    let login_id = credential_string(credentials, "loginId")?;
                    let api_token = credential_string(credentials, "apiToken")?;
                    Arc::new(crate::dns_dnspod::DnspodDns01Presenter::new(
                        client, login_id, api_token,
                    )?)
                }
                (DnsProviderKind::Cloudflare, serde_json::Value::Object(credentials)) => {
                    let client = crate::dns_http::DnsApiClient::new()?;
                    let api_token = credential_string(credentials, "apiToken")?;
                    Arc::new(crate::dns_cloudflare::CloudflareDns01Presenter::new(
                        client, api_token, None,
                    )?)
                }
                (DnsProviderKind::HttpRequest, credentials) => {
                    let client = crate::dns_http::DnsApiClient::new()?;
                    // The whole credential *is* the request configuration: this
                    // family has no fixed field names of its own, so the
                    // document round-trips back into the presenter verbatim.
                    let document = serde_json::to_string(credentials).map_err(|error| {
                        AcmeServiceError::validation(format!(
                            "HTTP_REQUEST credentials are not serializable: {error}"
                        ))
                    })?;
                    Arc::new(crate::dns_http_request::HttpRequestDns01Presenter::new(
                        client, &document,
                    )?)
                }
                (provider, _) => {
                    return Err(AcmeServiceError::validation(format!(
                        "credentials for {provider} must be an object with provider-specific keys"
                    )))
                }
            };
            registry.register(DnsCloudAccount {
                account_id: config.account_id.clone(),
                zone_apex: config.zone_apex.clone(),
                provider,
                presenter,
            })?;
        }
        Ok(registry)
    }

    /// True when every identifier resolves to an associated account.
    pub fn covers_all<'a, I, S>(&self, hostnames: I) -> bool
    where
        I: IntoIterator<Item = &'a S>,
        S: AsRef<str> + 'a + ?Sized,
    {
        hostnames
            .into_iter()
            .all(|hostname| self.resolve(hostname.as_ref()).is_some())
    }

    /// The shared presenter that dispatches per record to the owning
    /// account. All identifiers of a certificate share it; per-authorization
    /// zone resolution happens through [`Self::zone_for`].
    pub fn dispatch_presenter(self: &Arc<Self>) -> Arc<DispatchingDns01Presenter> {
        Arc::new(DispatchingDns01Presenter {
            registry: Arc::clone(self),
        })
    }

    /// Probes every associated account, in registration order.
    ///
    /// The registry is where the accounts are known, so it is where they can be
    /// checked as a set. One account's failure never stops the next: an operator
    /// fixing three credentials should learn about all three from one run, not
    /// about the first and then the second after another restart.
    pub async fn verify_accounts(&self) -> Vec<DnsAccountVerificationReport> {
        let mut reports = Vec::with_capacity(self.accounts.len());
        for account in &self.accounts {
            reports.push(DnsAccountVerificationReport {
                account_id: account.account_id.clone(),
                provider: account.provider,
                zone_apex: account.zone_apex.clone(),
                outcome: account.presenter.verify_account(&account.zone_apex).await,
            });
        }
        reports
    }
}

/// What probing one configured account established.
///
/// Carries the account identity alongside the outcome because the caller's next
/// step is to tell the operator *which* account is wrong, and a bare error text
/// does not say that.
#[derive(Debug)]
pub struct DnsAccountVerificationReport {
    pub account_id: String,
    pub provider: DnsProviderKind,
    pub zone_apex: String,
    /// The provider's own refusal when the account was rejected, or the reason
    /// no check was possible.
    pub outcome: AcmeServiceResult<DnsAccountVerification>,
}

impl DnsAccountVerificationReport {
    /// True when the provider actively refused the account, as opposed to not
    /// being asked.
    ///
    /// The distinction matters to a caller deciding whether to fail a startup:
    /// a refusal is a configuration mistake worth stopping for, while an
    /// unverifiable family is not a problem at all.
    pub fn is_rejected(&self) -> bool {
        self.outcome.is_err()
    }

    /// A single operator-facing line, with the provider's own words when it
    /// refused.
    pub fn describe(&self) -> String {
        let header = format!(
            "{} account `{}` for zone {}",
            self.provider, self.account_id, self.zone_apex
        );
        match &self.outcome {
            Ok(verification) => format!("{header}: {}", verification.describe()),
            Err(error) => format!("{header}: {error}"),
        }
    }
}

impl crate::dns_zone::DnsZoneResolver for DnsCloudAccountRegistry {
    fn zone_for(&self, identifier: &str) -> Option<String> {
        self.zone_for(identifier)
    }
}

impl DnsCloudAccountRegistry {
    /// Longest-suffix zone for one certificate identifier (`None` when no
    /// associated account covers it).
    pub fn zone_for(&self, identifier: &str) -> Option<String> {
        self.resolve(identifier)
            .map(|account| account.zone_apex.clone())
    }
}

/// DNS-01 presenter that routes each record to the account owning the
/// request's zone. The record request already carries its resolved
/// `zone_apex`; an unknown zone is a hard error (a TXT record published to
/// the wrong account would never validate).
pub struct DispatchingDns01Presenter {
    registry: Arc<DnsCloudAccountRegistry>,
}

impl DispatchingDns01Presenter {
    fn account_for(&self, zone_apex: &str) -> Result<&DnsCloudAccount, AcmeServiceError> {
        self.registry
            .accounts
            .iter()
            .find(|account| account.zone_apex == zone_apex)
            .ok_or_else(|| {
                AcmeServiceError::validation(format!(
                    "no cloud account is associated with zone {zone_apex}"
                ))
            })
    }
}

#[async_trait::async_trait]
impl Dns01Presenter for DispatchingDns01Presenter {
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        self.account_for(&request.zone_apex)?
            .presenter
            .publish(request)
            .await
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        self.account_for(&handle.zone_apex)?
            .presenter
            .withdraw(handle)
            .await
    }

    /// Probes the account that owns `zone_apex`.
    ///
    /// Routing by the zone rather than probing every account keeps the meaning
    /// identical to `publish`: the account asked about is the account that would
    /// have been used.
    async fn verify_account(&self, zone_apex: &str) -> AcmeServiceResult<DnsAccountVerification> {
        self.account_for(zone_apex)?
            .presenter
            .verify_account(zone_apex)
            .await
    }
}

fn credential_string(
    credentials: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> AcmeServiceResult<String> {
    credentials
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            AcmeServiceError::validation(format!(
                "cloud account credentials must contain non-empty `{key}`"
            ))
        })
}

/// Maximum accepted size of a DNS accounts file (64 KiB).
pub const MAX_DNS_ACCOUNTS_FILE_BYTES: u64 = 64 * 1024;

/// Reads and validates the DNS provider account file.
///
/// The file is the deployment's *only* source of DNS-01 credentials, so every
/// failure mode is reported as a configuration error naming the file: a
/// malformed file that silently degraded to "no accounts" would disable every
/// wildcard renewal without a single log line pointing at the cause.
///
/// Credentials are read from a file rather than an inline config value on
/// purpose — the configuration surface references secrets by path and never
/// embeds them.
pub fn load_dns_account_configs(
    path: &std::path::Path,
) -> AcmeServiceResult<Vec<DnsCloudAccountConfig>> {
    let describe = |detail: String| {
        AcmeServiceError::config(format!(
            "ACME DNS accounts file {} is invalid: {detail}",
            path.display()
        ))
    };
    let metadata =
        std::fs::metadata(path).map_err(|error| describe(format!("cannot be read ({error})")))?;
    if !metadata.is_file() {
        return Err(describe("not a regular file".to_string()));
    }
    if metadata.len() > MAX_DNS_ACCOUNTS_FILE_BYTES {
        return Err(describe(format!(
            "exceeds the {MAX_DNS_ACCOUNTS_FILE_BYTES} byte limit"
        )));
    }
    let raw = std::fs::read(path).map_err(|error| describe(format!("cannot be read ({error})")))?;
    let configs: Vec<DnsCloudAccountConfig> =
        serde_json::from_slice(&raw).map_err(|error| describe(error.to_string()))?;
    // Build the registry now, not on the first issuance: a broken credential or
    // a duplicate zone is a startup problem an operator can act on, not a
    // certificate failure that surfaces hours later in a renewal window.
    DnsCloudAccountRegistry::from_configs(&configs)?;
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::{DnsProviderKind, InMemoryDns01Presenter};

    fn account(zone: &str, id: &str) -> DnsCloudAccount {
        DnsCloudAccount {
            account_id: id.to_owned(),
            zone_apex: zone.to_owned(),
            provider: DnsProviderKind::Cloudflare,
            presenter: Arc::new(InMemoryDns01Presenter::default()),
        }
    }

    #[test]
    fn longest_zone_suffix_wins() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account("example.com", "apex"))
            .expect("apex");
        registry
            .register(account("dev.example.com", "dev"))
            .expect("dev zone");
        assert_eq!(
            registry.zone_for("www.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            registry.zone_for("api.dev.example.com").as_deref(),
            Some("dev.example.com")
        );
        assert_eq!(registry.zone_for("other.org").as_deref(), None);
    }

    #[test]
    fn wildcard_identifiers_resolve_to_base_zone() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account("example.com", "apex"))
            .expect("apex");
        assert_eq!(
            registry.zone_for("*.example.com").as_deref(),
            Some("example.com")
        );
        assert!(registry.covers_all(&["*.example.com", "example.com"]));
    }

    #[test]
    fn duplicate_zone_is_rejected() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account("example.com", "first"))
            .expect("first");
        assert!(registry.register(account("example.com", "second")).is_err());
    }

    #[test]
    fn covers_all_requires_every_identifier() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account("example.com", "apex"))
            .expect("apex");
        assert!(registry.covers_all(&["example.com", "*.example.com"]));
        assert!(!registry.covers_all(&["example.com", "other.org"]));
    }

    #[test]
    fn from_configs_builds_and_fails_closed_on_bad_credentials() {
        let configs = vec![
            DnsCloudAccountConfig {
                account_id: "cf-main".to_owned(),
                provider: "CLOUDFLARE".to_owned(),
                zone_apex: "example.com".to_owned(),
                credentials: serde_json::json!({ "apiToken": "token" }),
            },
            DnsCloudAccountConfig {
                account_id: "aliyun-dev".to_owned(),
                provider: "ALIYUN_DNS".to_owned(),
                zone_apex: "dev.example.com".to_owned(),
                credentials: serde_json::json!({
                    "accessKeyId": "id",
                    "accessKeySecret": "secret"
                }),
            },
        ];
        let registry = DnsCloudAccountRegistry::from_configs(&configs).expect("registry builds");
        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.zone_for("api.dev.example.com").as_deref(),
            Some("dev.example.com")
        );
        let broken = vec![DnsCloudAccountConfig {
            account_id: "cf-broken".to_owned(),
            provider: "CLOUDFLARE".to_owned(),
            zone_apex: "example.com".to_owned(),
            credentials: serde_json::json!({}),
        }];
        assert!(
            DnsCloudAccountRegistry::from_configs(&broken).is_err(),
            "missing credentials must fail closed"
        );
    }

    #[tokio::test]
    async fn dispatching_presenter_routes_by_zone_and_rejects_unknown() {
        let registry = Arc::new(DnsCloudAccountRegistry::empty());
        let presenter = registry.dispatch_presenter();
        let unknown = Dns01RecordRequest::new(
            "unassociated.zone",
            "_acme-challenge.unassociated.zone",
            "value",
        )
        .expect("request");
        assert!(presenter.publish(&unknown).await.is_err());
    }

    /// A presenter that refuses an account the way a misconfigured provider does.
    struct RejectingPresenter(&'static str);

    /// A presenter whose provider answers its read-only probe.
    struct VerifyingPresenter;

    #[async_trait::async_trait]
    impl Dns01Presenter for VerifyingPresenter {
        async fn publish(
            &self,
            _request: &Dns01RecordRequest,
        ) -> AcmeServiceResult<Dns01RecordHandle> {
            Err(AcmeServiceError::provider("unused in this test"))
        }

        async fn withdraw(&self, _handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
            Ok(())
        }

        async fn verify_account(
            &self,
            _zone_apex: &str,
        ) -> AcmeServiceResult<DnsAccountVerification> {
            Ok(DnsAccountVerification::Verified)
        }
    }

    #[async_trait::async_trait]
    impl Dns01Presenter for RejectingPresenter {
        async fn publish(
            &self,
            _request: &Dns01RecordRequest,
        ) -> AcmeServiceResult<Dns01RecordHandle> {
            Err(AcmeServiceError::provider("unused in this test"))
        }

        async fn withdraw(&self, _handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
            Ok(())
        }

        async fn verify_account(
            &self,
            zone_apex: &str,
        ) -> AcmeServiceResult<DnsAccountVerification> {
            Err(AcmeServiceError::provider(format!(
                "{} for {zone_apex}",
                self.0
            )))
        }
    }

    fn account_with(zone: &str, id: &str, presenter: Arc<dyn Dns01Presenter>) -> DnsCloudAccount {
        DnsCloudAccount {
            account_id: id.to_owned(),
            zone_apex: zone.to_owned(),
            provider: DnsProviderKind::Cloudflare,
            presenter,
        }
    }

    /// The report has to name the account, not just the failure: an operator with
    /// three zones needs to know which one the provider refused.
    #[tokio::test]
    async fn a_rejected_account_is_reported_with_its_identity_and_the_provider_error() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account_with(
                "example.com",
                "cf-main",
                Arc::new(RejectingPresenter(
                    "CLOUDFLARE rejected the request with HTTP 401: Authentication error (10000)",
                )),
            ))
            .expect("register");

        let reports = registry.verify_accounts().await;
        assert_eq!(reports.len(), 1);
        assert!(reports[0].is_rejected());
        assert_eq!(reports[0].account_id, "cf-main");
        assert_eq!(reports[0].zone_apex, "example.com");
        let line = reports[0].describe();
        assert!(line.contains("Authentication error"), "{line}");
        assert!(line.contains("cf-main"), "{line}");
        assert!(line.contains("example.com"), "{line}");
    }

    /// One broken account must not hide the others: an operator fixing three
    /// credentials should learn about all three from one run.
    #[tokio::test]
    async fn one_rejected_account_does_not_hide_the_others() {
        let mut registry = DnsCloudAccountRegistry::empty();
        registry
            .register(account_with(
                "ok.example",
                "healthy",
                Arc::new(VerifyingPresenter),
            ))
            .expect("register healthy");
        registry
            .register(account_with(
                "bad.example",
                "broken",
                Arc::new(RejectingPresenter("provider refused the credential")),
            ))
            .expect("register broken");
        registry
            .register(account("unverifiable.example", "no-read-only-call"))
            .expect("register unverifiable");
        let reports = registry.verify_accounts().await;
        assert_eq!(reports.len(), 3);
        assert!(!reports[0].is_rejected());
        assert!(reports[0].outcome.as_ref().expect("ok").is_verified());
        assert!(reports[1].is_rejected());
        assert!(reports[1]
            .describe()
            .contains("provider refused the credential"));
        // A family with no read-only probe reports "could not check", never
        // "checked and fine".
        assert!(!reports[2].is_rejected());
        assert!(!reports[2].outcome.as_ref().expect("ok").is_verified());
        assert!(reports[2].describe().contains("no read-only call"));
    }

    #[test]
    fn dns_accounts_file_round_trips_and_validates_at_load_time() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("dns-accounts.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!([
                {
                    "accountId": "cf-main",
                    "provider": "CLOUDFLARE",
                    "zoneApex": "example.com",
                    "credentials": { "apiToken": "token" }
                }
            ]))
            .expect("serialize"),
        )
        .expect("write");

        let configs = load_dns_account_configs(&path).expect("load");
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].zone_apex, "example.com");
        let registry = DnsCloudAccountRegistry::from_configs(&configs).expect("registry");
        assert_eq!(
            registry.zone_for("*.example.com").as_deref(),
            Some("example.com")
        );
    }

    /// Every failure mode names the file. A silently-empty registry would
    /// disable DNS-01 — and with it every wildcard renewal — with no clue.
    #[test]
    fn dns_accounts_file_failures_are_configuration_errors_naming_the_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let missing = directory.path().join("absent.json");
        let error = load_dns_account_configs(&missing).expect_err("missing file");
        assert!(matches!(error, AcmeServiceError::Config(_)));
        assert!(error.to_string().contains("absent.json"), "{error}");

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, b"{ not json").expect("write");
        let error = load_dns_account_configs(&malformed).expect_err("malformed json");
        assert!(error.to_string().contains("malformed.json"), "{error}");

        // A missing credential is a startup failure, not a first-issuance one.
        let incomplete = directory.path().join("incomplete.json");
        std::fs::write(
            &incomplete,
            br#"[{"accountId":"cf","provider":"CLOUDFLARE","zoneApex":"example.com","credentials":{}}]"#,
        )
        .expect("write");
        let error = load_dns_account_configs(&incomplete).expect_err("missing credential");
        assert!(error.to_string().contains("apiToken"), "{error}");

        // Unknown fields are rejected rather than ignored: a typo in a
        // credential key must not look like a successfully configured account.
        let typo = directory.path().join("typo.json");
        std::fs::write(
            &typo,
            br#"[{"accountId":"cf","provider":"CLOUDFLARE","zoneApex":"example.com","credentials":{"apiToken":"t"},"extra":1}]"#,
        )
        .expect("write");
        assert!(load_dns_account_configs(&typo).is_err());

        // Two entries for one zone is a configuration mistake: one zone has one
        // authoritative account.
        let duplicate = directory.path().join("duplicate.json");
        std::fs::write(
            &duplicate,
            br#"[{"accountId":"a","provider":"CLOUDFLARE","zoneApex":"example.com","credentials":{"apiToken":"t"}},
                 {"accountId":"b","provider":"CLOUDFLARE","zoneApex":"example.com","credentials":{"apiToken":"t"}}]"#,
        )
        .expect("write");
        let error = load_dns_account_configs(&duplicate).expect_err("duplicate zone");
        assert!(error.to_string().contains("already associated"), "{error}");

        // A directory where a file is expected must not be read as empty.
        assert!(load_dns_account_configs(directory.path()).is_err());
    }

    /// An empty list is legal and means "no DNS accounts": the certificate path
    /// then reports the actionable wildcard error rather than pretending DNS-01
    /// is configured.
    #[test]
    fn an_empty_dns_accounts_file_is_an_empty_registry_not_an_error() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("empty.json");
        std::fs::write(&path, b"[]").expect("write");
        let configs = load_dns_account_configs(&path).expect("empty list loads");
        assert!(configs.is_empty());
        assert!(DnsCloudAccountRegistry::from_configs(&configs)
            .expect("registry")
            .is_empty());
    }
}
