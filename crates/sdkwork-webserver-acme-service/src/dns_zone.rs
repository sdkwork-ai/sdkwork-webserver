//! Zone resolution seam for DNS-01: maps a certificate identifier to the
//! hosted zone that must carry its `_acme-challenge` record. Implemented by
//! the cloud-account registry (longest zone-suffix match) and by single-zone
//! adapters.

/// Resolves the hosted zone apex for one certificate identifier.
pub trait DnsZoneResolver: Send + Sync {
    /// `Some(zone_apex)` when an associated account covers the identifier.
    fn zone_for(&self, identifier: &str) -> Option<String>;
}

/// Single-zone adapter: every identifier resolves to one fixed zone. Used by
/// deployments whose certificates never span hosted zones.
#[derive(Clone, Debug)]
pub struct SingleZoneResolver {
    pub zone_apex: String,
}

impl DnsZoneResolver for SingleZoneResolver {
    fn zone_for(&self, _identifier: &str) -> Option<String> {
        Some(self.zone_apex.clone())
    }
}

/// Adapter over any present-when-needed zone lookup (e.g. a registry that
/// may be empty). `None` resolution means "no associated account" and the
/// caller fails the authorization with an explicit error.
pub struct OptionZoneResolver<F: Fn(&str) -> Option<String> + Send + Sync>(pub F);

impl<F: Fn(&str) -> Option<String> + Send + Sync> DnsZoneResolver for OptionZoneResolver<F> {
    fn zone_for(&self, identifier: &str) -> Option<String> {
        (self.0)(identifier)
    }
}
