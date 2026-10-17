//! Gateway-side tunnel security: bearer-token authentication, per-peer
//! authentication rate limiting, and route ownership checks (PRD §25, §44,
//! §46, §107).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sdkwork_webserver_tunnel_core::{DeviceId, Result, RouteId, SessionId, TunnelError};

pub mod acl;

pub use acl::{AclDecision, RouteAcl};

/// Verifies agent bearer tokens against the token set resolved from the
/// configured environment variables at gateway startup (PRD §40: secrets
/// never live in configuration files).
#[derive(Debug, Clone)]
pub struct TokenAuthenticator {
    entries: Vec<TokenEntry>,
}

/// One accepted credential and the user it speaks for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenEntry {
    token: String,
    /// `None` for a shared token: it authenticates a machine but speaks for no
    /// user, so the session is unlabelled and the global limits apply.
    user: Option<String>,
}

impl TokenAuthenticator {
    /// Builds the authenticator from already-resolved shared tokens.
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            entries: tokens
                .into_iter()
                .filter(|token| !token.is_empty())
                .map(|token| TokenEntry { token, user: None })
                .collect(),
        }
    }

    /// Builds the authenticator from shared tokens plus per-user ones.
    ///
    /// A user-bound token is what turns FRP's `user` label into a boundary: the
    /// session's user comes from the credential, so an agent cannot claim a
    /// name it holds no token for, and per-user quotas cannot be escaped by
    /// relabelling.
    pub fn with_users(tokens: Vec<String>, users: Vec<(String, String)>) -> Self {
        let mut authenticator = Self::new(tokens);
        for (user, token) in users {
            let token = token.trim().to_owned();
            let user = user.trim().to_owned();
            if token.is_empty() || user.is_empty() {
                continue;
            }
            authenticator.entries.push(TokenEntry {
                token,
                user: Some(user),
            });
        }
        authenticator
    }

    /// Resolves tokens from environment variable names; unset or empty
    /// variables contribute nothing. Returns an authenticator even when no
    /// variable is set, but [`Self::verify`] then fails closed.
    pub fn from_env(env_names: &[String]) -> Self {
        Self::new(resolve_env_tokens(env_names))
    }

    /// True when at least one token is configured; a gateway without any
    /// token refuses every agent (fail closed).
    pub fn is_configured(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Verifies a presented token in constant time per comparison.
    pub fn verify(&self, presented: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| sdkwork_utils_rust::crypto::secure_compare(&entry.token, presented))
    }

    /// The user this token speaks for, when it is a user-bound credential.
    ///
    /// Every entry is compared in constant time, and only the user is returned:
    /// the caller never learns which token matched, only whose it is.
    pub fn user_for(&self, presented: &str) -> Option<&str> {
        let mut matched: Option<&str> = None;
        for entry in &self.entries {
            if sdkwork_utils_rust::crypto::secure_compare(&entry.token, presented) {
                if entry.user.is_some() {
                    matched = entry.user.as_deref();
                }
            }
        }
        matched
    }

    /// Verifies a token and ties it to the identity the agent claims.
    ///
    /// Two rules, both fail-closed:
    ///
    /// - an unknown token is refused;
    /// - an agent that claims a user must hold a token bound to **that** user.
    ///   A shared token may not claim any user at all, because accepting the
    ///   claim would let one machine spend another team's quota and appear in
    ///   that team's listings.
    pub fn authenticate(
        &self,
        device_id: &DeviceId,
        token: &str,
        claimed_user: Option<&str>,
    ) -> Result<()> {
        if !self.verify(token) {
            tracing::warn!(%device_id, "tunnel authentication failed");
            return Err(TunnelError::AuthenticationFailed);
        }
        if let Some(claimed) = claimed_user.map(str::trim).filter(|user| !user.is_empty()) {
            if self.user_for(token) != Some(claimed) {
                tracing::warn!(
                    %device_id,
                    claimed,
                    "tunnel authentication refused: the token does not speak for that user"
                );
                return Err(TunnelError::AuthenticationFailed);
            }
        }
        Ok(())
    }

    /// The user a verified session belongs to, resolved from its token.
    pub fn user_of(&self, token: &str) -> Option<String> {
        self.user_for(token).map(str::to_owned)
    }
}

/// Resolves comma-separated tokens from environment variable names.
///
/// Public because the gateway composes shared tokens and per-user ones in one
/// authenticator: both must resolve the same way, or a credential would work
/// on one path and not the other.
pub fn resolve_env_tokens(env_names: &[String]) -> Vec<String> {
    env_names
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Fixed-window authentication attempt limiter per peer IP (PRD §107 DoS
/// protection). Locks are held only for map access, never across awaits.
#[derive(Debug)]
pub struct AuthRateLimiter {
    max_attempts: u32,
    window: Duration,
    attempts: Mutex<HashMap<IpAddr, AttemptWindow>>,
}

#[derive(Debug)]
struct AttemptWindow {
    count: u32,
    started_at: Instant,
}

impl AuthRateLimiter {
    /// Builds a limiter allowing `max_attempts` failures per `window`.
    pub fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            max_attempts,
            window,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Reports whether the peer may attempt authentication now, without
    /// consuming the failure budget: successful handshakes must never be
    /// charged (a fleet reconnecting behind one NAT gateway would otherwise
    /// lock itself out). Failure accounting lives in [`Self::record_failure`].
    pub fn check(&self, peer: IpAddr) -> bool {
        let attempts = self
            .attempts
            .lock()
            .expect("auth rate limiter map lock is never poisoned across awaits");
        match attempts.get(&peer) {
            Some(entry) if Instant::now().duration_since(entry.started_at) <= self.window => {
                entry.count < self.max_attempts
            }
            // No window, or an expired one: the peer may try again.
            _ => true,
        }
    }

    /// Records a failed attempt; returns false when the peer exceeded the
    /// failure budget and must back off.
    pub fn record_failure(&self, peer: IpAddr) -> bool {
        let mut attempts = self
            .attempts
            .lock()
            .expect("auth rate limiter map lock is never poisoned across awaits");
        let window = self.window;
        let now = Instant::now();
        let entry = attempts.entry(peer).or_insert(AttemptWindow {
            count: 0,
            started_at: now,
        });
        if now.duration_since(entry.started_at) > window {
            entry.count = 0;
            entry.started_at = now;
        }
        entry.count += 1;
        entry.count <= self.max_attempts
    }

    /// Drops expired windows; called opportunistically after each decision.
    pub fn sweep(&self) {
        let mut attempts = self
            .attempts
            .lock()
            .expect("auth rate limiter map lock is never poisoned across awaits");
        let window = self.window;
        let now = Instant::now();
        attempts.retain(|_, entry| now.duration_since(entry.started_at) <= window);
    }
}

/// Verifies that a session owns a route before mutation (PRD §44 route
/// ownership).
pub fn require_route_owner(
    route_owner: Option<&SessionId>,
    requester: &SessionId,
    route_id: &RouteId,
) -> Result<()> {
    match route_owner {
        Some(owner) if owner == requester => Ok(()),
        Some(_) => {
            tracing::warn!(
                %route_id,
                requester = %requester,
                "route ownership check failed"
            );
            Err(TunnelError::AuthorizationDenied)
        }
        None => Err(TunnelError::RouteNotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticator_verifies_tokens() {
        let authenticator = TokenAuthenticator::new(vec!["alpha".to_owned(), "beta".to_owned()]);
        assert!(authenticator.is_configured());
        assert!(authenticator.verify("beta"));
        assert!(!authenticator.verify("gamma"));
        let device = DeviceId::parse("dev_test").expect("valid id");
        assert!(authenticator.authenticate(&device, "alpha", None).is_ok());
        assert!(authenticator.authenticate(&device, "nope", None).is_err());
        // A shared token speaks for no user, so it may not claim one: accepting
        // the claim would let any machine spend another team's quota.
        assert!(authenticator
            .authenticate(&device, "alpha", Some("team-a"))
            .is_err());
    }

    /// Per-user credentials: the user comes from the token, never from the
    /// claim, and a mismatching claim is refused.
    #[test]
    fn user_bound_tokens_decide_the_user_and_refuse_a_foreign_claim() {
        let authenticator = TokenAuthenticator::with_users(
            vec!["shared".to_owned()],
            vec![
                ("team-a".to_owned(), "token-a".to_owned()),
                ("team-b".to_owned(), "token-b".to_owned()),
            ],
        );
        assert!(authenticator.verify("token-a"));
        assert_eq!(authenticator.user_for("token-a"), Some("team-a"));
        assert_eq!(authenticator.user_for("shared"), None);
        assert_eq!(authenticator.user_for("unknown"), None);

        let device = DeviceId::parse("dev_team").expect("valid id");
        assert!(authenticator
            .authenticate(&device, "token-a", Some("team-a"))
            .is_ok());
        // Right token, wrong team.
        assert!(authenticator
            .authenticate(&device, "token-a", Some("team-b"))
            .is_err());
        // Right team, wrong token.
        assert!(authenticator
            .authenticate(&device, "token-b", Some("team-a"))
            .is_err());
        // No claim at all is fine: the session is simply labelled by its token.
        assert!(authenticator.authenticate(&device, "token-a", None).is_ok());
        assert_eq!(authenticator.user_of("token-a").as_deref(), Some("team-a"));
        assert_eq!(authenticator.user_of("shared"), None);
    }

    #[test]
    fn authenticator_without_tokens_fails_closed() {
        let authenticator = TokenAuthenticator::new(vec![String::new()]);
        assert!(!authenticator.is_configured());
        let device = DeviceId::parse("dev_test").expect("valid id");
        assert!(authenticator.authenticate(&device, "", None).is_err());
    }

    #[test]
    fn rate_limiter_blocks_after_budget() {
        let limiter = AuthRateLimiter::new(3, Duration::from_secs(60));
        let peer: IpAddr = "10.1.1.1".parse().expect("ip");
        assert!(limiter.check(peer), "fresh peer may attempt");
        assert!(limiter.record_failure(peer), "first attempt allowed");
        assert!(limiter.record_failure(peer), "second attempt allowed");
        assert!(limiter.record_failure(peer), "third attempt allowed");
        assert!(!limiter.record_failure(peer), "fourth attempt blocked");
        assert!(!limiter.check(peer), "locked peer is refused before auth");
        // A different peer is unaffected.
        let other: IpAddr = "10.1.1.2".parse().expect("ip");
        assert!(limiter.record_failure(other));
        assert!(limiter.check(other));
        limiter.sweep();
    }

    #[test]
    fn rate_limiter_check_does_not_consume_budget() {
        let limiter = AuthRateLimiter::new(2, Duration::from_secs(60));
        let peer: IpAddr = "10.9.9.9".parse().expect("ip");
        // Any number of checks (successful handshakes) never consumes budget.
        for _ in 0..10 {
            assert!(limiter.check(peer));
        }
        assert!(limiter.record_failure(peer), "first failure allowed");
        assert!(limiter.record_failure(peer), "second failure allowed");
        assert!(!limiter.check(peer), "budget exhausted by failures only");
    }

    #[test]
    fn ownership_requires_matching_session() {
        let owner = SessionId::parse("session_a").expect("valid id");
        let route = RouteId::parse("route_x").expect("valid id");
        assert!(require_route_owner(Some(&owner), &owner, &route).is_ok());
        let other = SessionId::parse("session_b").expect("valid id");
        assert!(require_route_owner(Some(&owner), &other, &route).is_err());
        assert!(require_route_owner(None, &owner, &route).is_err());
    }

    #[test]
    fn token_verification_compares_full_lengths() {
        let authenticator = TokenAuthenticator::new(vec!["abc".to_owned()]);
        assert!(authenticator.verify("abc"));
        assert!(!authenticator.verify("abd"));
        assert!(!authenticator.verify("abcd"));
        assert!(!authenticator.verify(""));
    }
}
