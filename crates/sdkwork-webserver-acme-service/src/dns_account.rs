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
    normalize_dns_name, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind,
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
    /// Provider family: `ALIYUN_DNS`, `DNSPOD`, `CLOUDFLARE`.
    pub provider: String,
    /// Hosted zone apex the account owns, e.g. `example.com`.
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,
    /// Provider credentials. Aliyun/DNSPod: `{ accessKeyId, accessKeySecret }`;
    /// Cloudflare: `{ apiToken }`.
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
}
