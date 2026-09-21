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
    tokens: Vec<String>,
}

impl TokenAuthenticator {
    /// Builds the authenticator from already-resolved tokens.
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            tokens: tokens
                .into_iter()
                .filter(|token| !token.is_empty())
                .collect(),
        }
    }

    /// Resolves tokens from environment variable names; unset or empty
    /// variables contribute nothing. Returns an authenticator even when no
    /// variable is set, but [`Self::verify`] then fails closed.
    pub fn from_env(env_names: &[String]) -> Self {
        let tokens = env_names
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
            .collect();
        Self::new(tokens)
    }

    /// True when at least one token is configured; a gateway without any
    /// token refuses every agent (fail closed).
    pub fn is_configured(&self) -> bool {
        !self.tokens.is_empty()
    }

    /// Verifies a presented token in constant time per comparison.
    pub fn verify(&self, presented: &str) -> bool {
        self.tokens
            .iter()
            .any(|candidate| constant_time_eq(candidate.as_bytes(), presented.as_bytes()))
    }

    /// Verifies a token and ties it to the device identity claim. V1 tokens
    /// are shared credentials; any valid token authenticates any declared
    /// device id. Device-bound credentials arrive with mTLS (P1).
    pub fn authenticate(&self, device_id: &DeviceId, token: &str) -> Result<()> {
        if self.verify(token) {
            Ok(())
        } else {
            tracing::warn!(%device_id, "tunnel authentication failed");
            Err(TunnelError::AuthenticationFailed)
        }
    }
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
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
        assert!(authenticator.authenticate(&device, "alpha").is_ok());
        assert!(authenticator.authenticate(&device, "nope").is_err());
    }

    #[test]
    fn authenticator_without_tokens_fails_closed() {
        let authenticator = TokenAuthenticator::new(vec![String::new()]);
        assert!(!authenticator.is_configured());
        let device = DeviceId::parse("dev_test").expect("valid id");
        assert!(authenticator.authenticate(&device, "").is_err());
    }

    #[test]
    fn rate_limiter_blocks_after_budget() {
        let limiter = AuthRateLimiter::new(3, Duration::from_secs(60));
        let peer: IpAddr = "10.1.1.1".parse().expect("ip");
        assert!(limiter.record_failure(peer), "first attempt allowed");
        assert!(limiter.record_failure(peer), "second attempt allowed");
        assert!(limiter.record_failure(peer), "third attempt allowed");
        assert!(!limiter.record_failure(peer), "fourth attempt blocked");
        // A different peer is unaffected.
        let other: IpAddr = "10.1.1.2".parse().expect("ip");
        assert!(limiter.record_failure(other));
        limiter.sweep();
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
    fn constant_time_eq_matches_only_equal_lengths_and_bytes() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
