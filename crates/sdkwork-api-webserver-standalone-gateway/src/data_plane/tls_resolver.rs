//! Layered SNI certificate resolution.
//!
//! A listener can serve two certificate sources at once: the operator's
//! configured certificate files and the certificate set assigned by the
//! control plane. Resolution happens per server name and is layered — the
//! configured source wins for a name when it covers that name and is currently
//! usable, and the assigned source covers every name the configured source
//! does not. A name that neither source covers gets no certificate, so an
//! unauthorized host still fails the handshake instead of silently being
//! served some other tenant's certificate.
//!
//! A certificate outside its own validity window never outranks a usable one,
//! but it is still served when nothing else covers the name. That keeps a host
//! reachable while its replacement certificate is still propagating, matching
//! the tolerance the file loader already applies to expired leaves.

use std::{collections::HashMap, sync::Arc};

use rustls::{
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use sdkwork_utils_rust::datetime;
use sdkwork_webserver_core::{
    normalize_server_name, normalize_tls_server_name, tls_runtime::CertificateSource,
    wildcard_server_name_covers,
};

/// A certificate plus the validity window used to rank it against another
/// source's certificate for the same server name.
#[derive(Debug)]
pub(crate) struct ResolvableCertificate {
    pub(crate) certified_key: Arc<CertifiedKey>,
    /// Lowercase SHA-256 of the leaf DER.
    ///
    /// Carried so the resolver can name the certificate it *would* serve
    /// without re-reading the file, which is what the served-certificate
    /// report is built from.
    pub(crate) fingerprint_sha256: String,
    pub(crate) not_before_unix_seconds: i64,
    pub(crate) not_after_unix_seconds: i64,
}

impl ResolvableCertificate {
    pub(crate) fn new(
        certified_key: Arc<CertifiedKey>,
        fingerprint_sha256: String,
        not_before_unix_seconds: i64,
        not_after_unix_seconds: i64,
    ) -> Self {
        Self {
            certified_key,
            fingerprint_sha256,
            not_before_unix_seconds,
            not_after_unix_seconds,
        }
    }

    /// Whether the certificate's own validity window contains the instant.
    ///
    /// This deliberately consults only the leaf's `notBefore`/`notAfter`; it
    /// says nothing about whether a client would trust the chain. Ranking is
    /// about picking the least-bad option for a host, not about acceptance.
    pub(crate) fn is_usable_at(&self, now_unix_seconds: i64) -> bool {
        self.not_before_unix_seconds <= now_unix_seconds
            && now_unix_seconds <= self.not_after_unix_seconds
    }
}

/// Server-name index for a single certificate source.
///
/// Exact names and wildcards are kept apart so an exact match can outrank any
/// wildcard. The wildcards are held unordered because a queried name can match
/// at most one of them: a wildcard pins the number of labels below the suffix,
/// so `*.a.test` and `*.test` cover disjoint sets of names. No lookup order is
/// therefore observable, and no insertion can invalidate one.
#[derive(Debug, Clone, Default)]
pub(crate) struct CertificateSourceIndex {
    exact: HashMap<String, Arc<ResolvableCertificate>>,
    wildcards: Vec<(String, Arc<ResolvableCertificate>)>,
}

impl CertificateSourceIndex {
    /// Adds one certificate under every server name it declares.
    ///
    /// Returns a diagnostic naming the offending server name: either a
    /// duplicate claim inside this source, or a name that is not a valid TLS
    /// server name. Ambiguity within one source is an operator mistake, so it
    /// is rejected rather than resolved by insertion order.
    pub(crate) fn insert(
        &mut self,
        server_names: &[String],
        certificate: Arc<ResolvableCertificate>,
    ) -> Result<(), String> {
        for server_name in server_names {
            let normalized = normalize_tls_server_name(server_name)
                .ok_or_else(|| format!("invalid TLS server name {server_name}"))?;
            if let Some(suffix) = normalized.strip_prefix("*.") {
                if self
                    .wildcards
                    .iter()
                    .any(|(existing, _)| existing == suffix)
                {
                    return Err(normalized);
                }
                self.wildcards
                    .push((suffix.to_owned(), Arc::clone(&certificate)));
            } else if self
                .exact
                .insert(normalized.clone(), Arc::clone(&certificate))
                .is_some()
            {
                return Err(normalized);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.wildcards.is_empty()
    }

    /// The usable certificate covering `server_name`, an exact name first.
    ///
    /// A stale exact entry does not shadow a usable wildcard: the exact match
    /// only wins while it is inside its own validity window.
    pub(crate) fn select_usable(
        &self,
        server_name: &str,
        now_unix_seconds: i64,
    ) -> Option<&Arc<ResolvableCertificate>> {
        if let Some(certificate) = self.exact.get(server_name) {
            if certificate.is_usable_at(now_unix_seconds) {
                return Some(certificate);
            }
        }
        self.wildcard_covering(server_name, Some(now_unix_seconds))
    }

    /// The certificate covering `server_name`, ignoring validity.
    pub(crate) fn select_any(&self, server_name: &str) -> Option<&Arc<ResolvableCertificate>> {
        if let Some(certificate) = self.exact.get(server_name) {
            return Some(certificate);
        }
        self.wildcard_covering(server_name, None)
    }

    /// The wildcard covering `server_name`, optionally restricted to those
    /// inside their validity window.
    ///
    /// At most one wildcard can match, so this is a search rather than a
    /// ranking. `usable_at` selects the rule: [`Self::select_usable`] demands a
    /// certificate inside its window, while [`Self::select_any`] accepts one
    /// that merely covers the name so a host whose replacement is still
    /// propagating stays reachable.
    fn wildcard_covering(
        &self,
        server_name: &str,
        usable_at: Option<i64>,
    ) -> Option<&Arc<ResolvableCertificate>> {
        self.wildcards
            .iter()
            .find(|(suffix, certificate)| {
                wildcard_server_name_covers(suffix, server_name)
                    && usable_at.is_none_or(|now| certificate.is_usable_at(now))
            })
            .map(|(_, certificate)| certificate)
    }
}

/// The certificate sources a listener serves, in priority order.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolverLayers {
    policy: CertificateSourceIndex,
    assignment: CertificateSourceIndex,
}

impl ResolverLayers {
    pub(crate) fn policy_only(policy: CertificateSourceIndex) -> Self {
        Self {
            policy,
            assignment: CertificateSourceIndex::default(),
        }
    }

    pub(crate) fn assignment_only(assignment: CertificateSourceIndex) -> Self {
        Self {
            policy: CertificateSourceIndex::default(),
            assignment,
        }
    }

    pub(crate) fn new(policy: CertificateSourceIndex, assignment: CertificateSourceIndex) -> Self {
        Self { policy, assignment }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.policy.is_empty() && self.assignment.is_empty()
    }

    /// Selects the certificate for one server name together with its source.
    ///
    /// This is the single ordering definition. [`Self::select`] defers to it,
    /// so a handshake and a served-certificate report can never disagree about
    /// which certificate a name receives.
    ///
    /// Order: usable configured file, usable assigned certificate, then the
    /// configured file or assigned certificate that merely covers the name.
    /// The final two steps are what keep an expired host serving rather than
    /// going dark while its replacement is still propagating.
    pub(crate) fn select_with_source(
        &self,
        server_name: &str,
        now_unix_seconds: i64,
    ) -> Option<(CertificateSource, &Arc<ResolvableCertificate>)> {
        if let Some(certificate) = self.policy.select_usable(server_name, now_unix_seconds) {
            return Some((CertificateSource::Config, certificate));
        }
        if let Some(certificate) = self.assignment.select_usable(server_name, now_unix_seconds) {
            return Some((CertificateSource::Assignment, certificate));
        }
        if let Some(certificate) = self.policy.select_any(server_name) {
            return Some((CertificateSource::Config, certificate));
        }
        self.assignment
            .select_any(server_name)
            .map(|certificate| (CertificateSource::Assignment, certificate))
    }

    /// Selects the certificate for one server name.
    pub(crate) fn select(
        &self,
        server_name: &str,
        now_unix_seconds: i64,
    ) -> Option<&Arc<ResolvableCertificate>> {
        self.select_with_source(server_name, now_unix_seconds)
            .map(|(_, certificate)| certificate)
    }

    /// The assigned-certificate index, so a caller can compose a layer set
    /// that keeps this source in the lower position.
    pub(crate) fn assignments(&self) -> &CertificateSourceIndex {
        &self.assignment
    }
}

/// Resolves each TLS handshake against one immutable layer set.
///
/// A rotation publishes a whole new `ServerConfig` with a new resolver rather
/// than mutating a live one, so a handshake observes either the previous layer
/// set or the next one and never a half-applied combination. Rebuilding the
/// configuration also carries the rotation's own protocol parameters (minimum
/// and maximum TLS version, ALPN, client auth), which an in-place layer swap
/// could not express.
#[derive(Debug)]
pub(crate) struct LayeredCertificateResolver {
    layers: Arc<ResolverLayers>,
}

impl LayeredCertificateResolver {
    pub(crate) fn new(layers: Arc<ResolverLayers>) -> Self {
        Self { layers }
    }
}

impl ResolvesServerCert for LayeredCertificateResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        // Fail closed: a missing or unknown SNI gets no certificate, so the
        // handshake is rejected instead of serving a default certificate to a
        // host the operator never authorized.
        let server_name = normalize_server_name(client_hello.server_name()?)?;
        self.layers
            .select(&server_name, datetime::now().timestamp())
            .map(|certificate| Arc::clone(&certificate.certified_key))
    }
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, KeyPair};
    use rustls::pki_types::{pem::PemObject, PrivateKeyDer};

    use super::*;

    const NOW: i64 = 1_700_000_000;
    const VALID_FROM: i64 = NOW - 1_000;
    const VALID_UNTIL: i64 = NOW + 1_000;
    const STALE_FROM: i64 = NOW - 10_000;
    const STALE_UNTIL: i64 = NOW - 5_000;

    fn certificate(not_before: i64, not_after: i64) -> Arc<ResolvableCertificate> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let params = CertificateParams::new(vec!["site.example.test".to_owned()]).expect("params");
        let key = KeyPair::generate().expect("key pair");
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let private_key =
            PrivateKeyDer::from_pem_slice(key.serialize_pem().as_bytes()).expect("private key");
        let certified_key = CertifiedKey::from_der(
            vec![certificate.der().clone()],
            private_key,
            rustls::crypto::CryptoProvider::get_default().expect("installed provider"),
        )
        .expect("certified key");
        Arc::new(ResolvableCertificate::new(
            Arc::new(certified_key),
            "a".repeat(64),
            not_before,
            not_after,
        ))
    }

    fn index(entries: &[(&str, Arc<ResolvableCertificate>)]) -> CertificateSourceIndex {
        let mut index = CertificateSourceIndex::default();
        for (name, certificate) in entries {
            index
                .insert(&[(*name).to_owned()], Arc::clone(certificate))
                .expect("unambiguous server name");
        }
        index
    }

    fn layered(
        policy: &[(&str, Arc<ResolvableCertificate>)],
        assignment: &[(&str, Arc<ResolvableCertificate>)],
    ) -> ResolverLayers {
        ResolverLayers::new(index(policy), index(assignment))
    }

    fn assert_selected(
        layers: &ResolverLayers,
        server_name: &str,
        expected: &Arc<ResolvableCertificate>,
    ) {
        let selected = layers
            .select(server_name, NOW)
            .unwrap_or_else(|| panic!("{server_name} must resolve"));
        assert!(
            Arc::ptr_eq(&selected.certified_key, &expected.certified_key),
            "{server_name} resolved to an unexpected certificate"
        );
    }

    #[test]
    fn configured_file_wins_over_the_assigned_certificate() {
        let policy = certificate(VALID_FROM, VALID_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[("site.example.test", Arc::clone(&policy))],
            &[("site.example.test", Arc::clone(&assigned))],
        );
        assert_selected(&layers, "site.example.test", &policy);
    }

    #[test]
    fn stale_configured_file_yields_to_the_assigned_certificate() {
        let policy = certificate(STALE_FROM, STALE_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[("site.example.test", Arc::clone(&policy))],
            &[("site.example.test", Arc::clone(&assigned))],
        );
        assert_selected(&layers, "site.example.test", &assigned);
    }

    #[test]
    fn stale_configured_file_is_still_served_when_nothing_else_covers_the_name() {
        let policy = certificate(STALE_FROM, STALE_UNTIL);
        let layers = layered(&[("site.example.test", Arc::clone(&policy))], &[]);
        assert_selected(&layers, "site.example.test", &policy);
    }

    #[test]
    fn wildcard_only_covers_the_assigned_names_it_declares() {
        let policy = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(&[("*.example.test", Arc::clone(&policy))], &[]);
        assert_selected(&layers, "www.example.test", &policy);
        assert!(layers.select("example.test", NOW).is_none());
        assert!(layers.select("a.b.example.test", NOW).is_none());
    }

    #[test]
    fn usable_wildcard_outranks_a_stale_exact_entry() {
        let exact = certificate(STALE_FROM, STALE_UNTIL);
        let wildcard = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[
                ("site.example.test", Arc::clone(&exact)),
                ("*.example.test", Arc::clone(&wildcard)),
            ],
            &[],
        );
        assert_selected(&layers, "site.example.test", &wildcard);
    }

    #[test]
    fn exact_match_outranks_a_wildcard_within_one_source() {
        let exact = certificate(VALID_FROM, VALID_UNTIL);
        let wildcard = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[
                ("*.example.test", Arc::clone(&wildcard)),
                ("site.example.test", Arc::clone(&exact)),
            ],
            &[],
        );
        assert_selected(&layers, "site.example.test", &exact);
    }

    /// Precedence is derived from the queried name, not from the order the
    /// index was built in, so no insertion can invalidate it.
    #[test]
    fn wildcard_precedence_is_independent_of_insertion_order() {
        let broad = certificate(VALID_FROM, VALID_UNTIL);
        let specific = certificate(VALID_FROM, VALID_UNTIL);
        let broad_first = layered(
            &[
                ("*.test", Arc::clone(&broad)),
                ("*.example.test", Arc::clone(&specific)),
            ],
            &[],
        );
        let specific_first = layered(
            &[
                ("*.example.test", Arc::clone(&specific)),
                ("*.test", Arc::clone(&broad)),
            ],
            &[],
        );
        assert_selected(&broad_first, "www.example.test", &specific);
        assert_selected(&specific_first, "www.example.test", &specific);
        assert_selected(&broad_first, "www.test", &broad);
        assert_selected(&specific_first, "www.test", &broad);
    }

    /// Two wildcards can never compete for one name: each pins the number of
    /// labels below its suffix, so their names are disjoint. That is why the
    /// source index carries no ordering.
    #[test]
    fn wildcards_with_different_suffixes_cover_disjoint_names() {
        let narrow = certificate(VALID_FROM, VALID_UNTIL);
        let broad = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[
                ("*.example.test", Arc::clone(&narrow)),
                ("*.test", Arc::clone(&broad)),
            ],
            &[],
        );
        assert_selected(&layers, "www.example.test", &narrow);
        assert_selected(&layers, "www.test", &broad);
        // The broader wildcard covers one label above its own suffix too, which
        // is the name the narrower wildcard is anchored to. The two sets still
        // never meet: that name cannot also carry the extra label the narrower
        // wildcard requires.
        assert_selected(&layers, "example.test", &broad);
        // Neither wildcard reaches two labels above its suffix.
        assert!(layers.select("a.www.test", NOW).is_none());
        assert!(layers.select("a.b.example.test", NOW).is_none());
    }

    /// A stale wildcard still covers its own names once nothing usable competes,
    /// which is the tolerance the configured file layer already gives a host
    /// whose replacement is still propagating.
    #[test]
    fn a_stale_wildcard_is_still_served_when_no_usable_candidate_covers_the_name() {
        let stale = certificate(STALE_FROM, STALE_UNTIL);
        let layers = layered(&[("*.example.test", Arc::clone(&stale))], &[]);
        assert_selected(&layers, "www.example.test", &stale);
    }

    #[test]
    fn an_uncovered_server_name_resolves_to_nothing() {
        let policy = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(&[("site.example.test", Arc::clone(&policy))], &[]);
        assert!(layers.select("other.example.test", NOW).is_none());
    }

    #[test]
    fn one_source_cannot_claim_the_same_name_twice() {
        let first = certificate(VALID_FROM, VALID_UNTIL);
        let second = certificate(VALID_FROM, VALID_UNTIL);
        let mut index = CertificateSourceIndex::default();
        index
            .insert(&["site.example.test".to_owned()], first)
            .expect("first claim");
        let conflict = index
            .insert(&["site.example.test".to_owned()], second)
            .expect_err("second claim must be rejected");
        assert_eq!(conflict, "site.example.test");
    }

    #[test]
    fn the_two_sources_may_each_claim_the_same_name() {
        let policy = certificate(STALE_FROM, STALE_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[("site.example.test", Arc::clone(&policy))],
            &[("site.example.test", Arc::clone(&assigned))],
        );
        assert!(!layers.is_empty());
        assert_selected(&layers, "site.example.test", &assigned);
    }

    #[test]
    fn a_stale_configured_wildcard_yields_to_the_assigned_exact_certificate() {
        let policy = certificate(STALE_FROM, STALE_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[("*.example.test", Arc::clone(&policy))],
            &[("site.example.test", Arc::clone(&assigned))],
        );
        assert_selected(&layers, "site.example.test", &assigned);
    }

    #[test]
    fn a_usable_configured_wildcard_outranks_the_assigned_exact_certificate() {
        let policy = certificate(VALID_FROM, VALID_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let layers = layered(
            &[("*.example.test", Arc::clone(&policy))],
            &[("site.example.test", Arc::clone(&assigned))],
        );
        assert_selected(&layers, "site.example.test", &policy);
    }

    /// The served-certificate report must name the certificate a handshake
    /// receives *and* the source that provided it, including the case an
    /// external probe cannot distinguish: a configured file winning a name an
    /// assigned certificate also claims.
    #[test]
    fn selection_reports_the_source_that_won() {
        let configured = certificate(VALID_FROM, VALID_UNTIL);
        let assigned = certificate(VALID_FROM, VALID_UNTIL);
        let expired = certificate(STALE_FROM, STALE_UNTIL);

        let both = layered(
            &[("configured.example.test", Arc::clone(&configured))],
            &[("assigned.example.test", Arc::clone(&assigned))],
        );
        let (source, certificate) = both
            .select_with_source("configured.example.test", NOW)
            .expect("the configured name resolves");
        assert_eq!(source, CertificateSource::Config);
        assert_eq!(certificate.fingerprint_sha256, "a".repeat(64));

        let (source, _) = both
            .select_with_source("assigned.example.test", NOW)
            .expect("the assigned name resolves");
        assert_eq!(source, CertificateSource::Assignment);

        // A stale configured file is still served when nothing else covers the
        // name, so the report must attribute it to `config` rather than claim
        // the name has no certificate.
        let stale_only = layered(&[("stale.example.test", Arc::clone(&expired))], &[]);
        let (source, _) = stale_only
            .select_with_source("stale.example.test", NOW)
            .expect("a stale configured name is still served");
        assert_eq!(source, CertificateSource::Config);

        assert!(both
            .select_with_source("uncovered.example.test", NOW)
            .is_none());
        assert_eq!(CertificateSource::Config.as_str(), "config");
        assert_eq!(CertificateSource::Assignment.as_str(), "assignment");
    }
}
