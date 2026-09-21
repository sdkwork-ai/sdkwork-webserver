//! Gateway route registry (PRD §29, §56, §59).
//!
//! Domain-keyed and port-keyed indexes over registered routes. All
//! mutations validate ownership conflicts so two devices can never claim
//! the same public surface. Locks are held only for map access, never
//! across await points.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use sdkwork_webserver_tunnel_core::{Result, RouteId, SessionId, TunnelError, TunnelRoute};

/// One registered route with its owning session.
#[derive(Debug, Clone)]
pub struct RegisteredRoute {
    /// The validated route (carries `session_id` ownership).
    pub route: TunnelRoute,
}

impl RegisteredRoute {
    /// The owning session id.
    pub fn session(&self) -> Option<&SessionId> {
        self.route.session_id.as_ref()
    }
}

/// Thread-safe registry of live tunnel routes.
#[derive(Debug, Default)]
pub struct RouteRegistry {
    inner: RwLock<RegistryInner>,
    version: AtomicU64,
}

#[derive(Debug, Default)]
struct RegistryInner {
    by_domain: HashMap<String, Arc<RegisteredRoute>>,
    /// Wildcard domain routes keyed by their suffix (`*.a.b` is stored under
    /// `a.b`).
    by_wildcard: HashMap<String, Arc<RegisteredRoute>>,
    /// Port-keyed index: TCP and UDP port namespaces are independent, so the
    /// same port number may legitimately host one route of each protocol.
    by_port: HashMap<(bool, u16), Arc<RegisteredRoute>>,
}

impl RouteRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Monotonic registry version; changes on every mutation so dispatchers
    /// can detect staleness cheaply.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Registers a route, rejecting conflicts with a live route owned by a
    /// different session. Re-registering the same id from the owning
    /// session replaces the entry (hot update, PRD §55).
    pub fn register(&self, route: TunnelRoute) -> Result<Arc<RegisteredRoute>> {
        let Some(owner) = route.session_id.clone() else {
            return Err(TunnelError::InvalidRoute(
                "route must carry its owning session".to_owned(),
            ));
        };
        let mut inner = self
            .inner
            .write()
            .expect("route registry lock is never held across awaits");
        if let Some(domain) = domain_key(&route) {
            if let Some(existing) = inner.by_domain.get(domain) {
                if existing.session() != Some(&owner) {
                    return Err(TunnelError::RouteConflict(format!(
                        "domain {domain} is already served by another device"
                    )));
                }
            }
        }
        if let Some(suffix) = wildcard_key(&route) {
            if let Some(existing) = inner.by_wildcard.get(&suffix) {
                if existing.session() != Some(&owner) {
                    return Err(TunnelError::RouteConflict(format!(
                        "wildcard domain *.{suffix} is already served by another device"
                    )));
                }
            }
        }
        let datagram = is_datagram(&route);
        if let Some(port) = port_key(&route) {
            let key = (datagram, port);
            if let Some(existing) = inner.by_port.get(&key) {
                if existing.session() != Some(&owner) {
                    return Err(TunnelError::RouteConflict(format!(
                        "gateway port {port} is already served by another device"
                    )));
                }
            }
        }
        // Replacing an existing registration with the same id keeps counts
        // stable.
        let id = route.id.clone();
        if let Some(existing) = find_by_id(&inner, &id) {
            if let Some(domain) = domain_key(&existing.route) {
                inner.by_domain.remove(domain);
            }
            if let Some(suffix) = wildcard_key(&existing.route) {
                inner.by_wildcard.remove(&suffix);
            }
            if let Some(port) = port_key(&existing.route) {
                let key = (is_datagram(&existing.route), port);
                inner.by_port.remove(&key);
            }
        }
        let registered = Arc::new(RegisteredRoute { route });
        if let Some(suffix) = wildcard_key(&registered.route) {
            inner.by_wildcard.insert(suffix, registered.clone());
        } else if let Some(domain) = domain_key(&registered.route) {
            inner
                .by_domain
                .insert(domain.to_owned(), registered.clone());
        }
        if let Some(port) = port_key(&registered.route) {
            let key = (datagram, port);
            inner.by_port.insert(key, registered.clone());
        }
        self.version.fetch_add(1, Ordering::Relaxed);
        Ok(registered)
    }

    /// Removes a route by id; returns the removed route.
    pub fn unregister(&self, id: &RouteId) -> Option<Arc<RegisteredRoute>> {
        let mut inner = self
            .inner
            .write()
            .expect("route registry lock is never held across awaits");
        let removed = remove_by_id(&mut inner, id);
        if removed.is_some() {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// Removes every route owned by `session`; returns their ids (used on
    /// session teardown, PRD §29 route → session chain).
    pub fn remove_session(&self, session: &SessionId) -> Vec<RouteId> {
        let mut inner = self
            .inner
            .write()
            .expect("route registry lock is never held across awaits");
        let domain_ids: Vec<RouteId> = inner
            .by_domain
            .values()
            .filter(|route| route.session() == Some(session))
            .map(|route| route.route.id.clone())
            .collect();
        for id in &domain_ids {
            remove_by_id(&mut inner, id);
        }
        let wildcard_ids: Vec<RouteId> = inner
            .by_wildcard
            .values()
            .filter(|route| route.session() == Some(session))
            .map(|route| route.route.id.clone())
            .collect();
        for id in &wildcard_ids {
            remove_by_id(&mut inner, id);
        }
        let port_ids: Vec<RouteId> = inner
            .by_port
            .values()
            .filter(|route| route.session() == Some(session))
            .map(|route| route.route.id.clone())
            .collect();
        for id in &port_ids {
            remove_by_id(&mut inner, id);
        }
        let mut removed = domain_ids;
        removed.extend(wildcard_ids);
        removed.extend(port_ids);
        if !removed.is_empty() {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// Looks up one registered route by id.
    pub fn get(&self, id: &RouteId) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner
            .by_domain
            .values()
            .chain(inner.by_wildcard.values())
            .chain(inner.by_port.values())
            .find(|route| &route.route.id == id)
            .cloned()
    }

    /// Matches a visitor host (lowercase, port-less) to a live route. Exact
    /// registrations win over wildcards; among wildcards the longest suffix
    /// wins. A wildcard covers only subdomains (`*.a.b` does not match
    /// `a.b` itself).
    pub fn match_domain(&self, host: &str) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        if let Some(exact) = inner.by_domain.get(host) {
            return Some(exact.clone());
        }
        inner
            .by_wildcard
            .iter()
            .filter(|(suffix, _)| host.ends_with(&format!(".{suffix}")))
            .max_by_key(|(suffix, _)| suffix.len())
            .map(|(_, route)| route.clone())
    }

    /// Matches a gateway TCP listener port to a live route.
    pub fn match_port(&self, port: u16) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner_by_port(&inner, false, port)
    }

    /// Matches a gateway UDP listener port to a live route.
    pub fn match_udp_port(&self, port: u16) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner_by_port(&inner, true, port)
    }

    /// Every registered route, sorted by id for stable API output.
    pub fn list(&self) -> Vec<TunnelRoute> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        let mut routes: Vec<TunnelRoute> = inner
            .by_domain
            .values()
            .chain(inner.by_wildcard.values())
            .chain(inner.by_port.values())
            .map(|route| route.route.clone())
            .collect();
        routes.sort_by(|left, right| left.id.cmp(&right.id));
        routes.dedup_by(|left, right| left.id == right.id);
        routes
    }

    /// Number of registered routes.
    pub fn count(&self) -> usize {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner.by_domain.len() + inner.by_wildcard.len() + inner.by_port.len()
    }
}

fn domain_key(route: &TunnelRoute) -> Option<&str> {
    route
        .matcher
        .as_domain()
        .filter(|domain| !domain.starts_with("*."))
}

fn wildcard_key(route: &TunnelRoute) -> Option<String> {
    route.matcher.wildcard_suffix().map(str::to_owned)
}

fn is_datagram(route: &TunnelRoute) -> bool {
    route.protocol == sdkwork_webserver_tunnel_core::TunnelProtocolKind::Udp
}

fn inner_by_port(
    inner: &RegistryInner,
    datagram: bool,
    port: u16,
) -> Option<Arc<RegisteredRoute>> {
    inner.by_port.get(&(datagram, port)).cloned()
}

fn port_key(route: &TunnelRoute) -> Option<u16> {
    route.matcher.as_port()
}

fn find_by_id(inner: &RegistryInner, id: &RouteId) -> Option<RegisteredRoute> {
    inner
        .by_domain
        .values()
        .chain(inner.by_wildcard.values())
        .chain(inner.by_port.values())
        .find(|route| &route.route.id == id)
        .map(|route| (**route).clone())
}

fn remove_by_id(inner: &mut RegistryInner, id: &RouteId) -> Option<Arc<RegisteredRoute>> {
    let domain_removed = inner
        .by_domain
        .iter()
        .find(|(_, route)| &route.route.id == id)
        .map(|(key, _)| key.clone());
    if let Some(key) = domain_removed {
        return inner.by_domain.remove(&key);
    }
    let wildcard_removed = inner
        .by_wildcard
        .iter()
        .find(|(_, route)| &route.route.id == id)
        .map(|(key, _)| key.clone());
    if let Some(key) = wildcard_removed {
        return inner.by_wildcard.remove(&key);
    }
    let port_removed = inner
        .by_port
        .iter()
        .find(|(_, route)| &route.route.id == id)
        .map(|(key, _)| *key);
    if let Some(key) = port_removed {
        return inner.by_port.remove(&key);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sdkwork_webserver_tunnel_core::{
        local_target, RouteMatcher, RoutePolicy, TunnelProtocolKind,
    };

    fn sample_route(id: &str, domain: &str, session: &str) -> TunnelRoute {
        TunnelRoute::new(
            RouteId::parse(id).expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain(domain).expect("valid domain"),
            local_target(3000),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_session(SessionId::parse(session).expect("valid id"), Utc::now())
    }

    #[test]
    fn register_and_match_domain() {
        let registry = RouteRegistry::new();
        let owner = SessionId::parse("session_a").expect("valid id");
        let registered = registry
            .register(sample_route("route_a", "demo.sdkwork.link", "session_a"))
            .expect("register");
        assert_eq!(registered.session(), Some(&owner));
        assert!(
            registry.match_domain("demo.sdkwork.link").is_some(),
            "lowercase host matches"
        );
        assert!(registry.match_domain("other.sdkwork.link").is_none());
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.remove_session(&owner).len(), 1);
        assert!(registry.match_domain("demo.sdkwork.link").is_none());
    }

    #[test]
    fn conflicting_domain_is_rejected() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_a", "demo.sdkwork.link", "session_a"))
            .expect("register");
        let error = registry
            .register(sample_route("route_b", "demo.sdkwork.link", "session_b"))
            .expect_err("cross-session conflict");
        assert!(matches!(error, TunnelError::RouteConflict(_)));
    }

    #[test]
    fn same_session_replaces_registration() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_a", "demo.sdkwork.link", "session_a"))
            .expect("register");
        registry
            .register(sample_route("route_a", "demo2.sdkwork.link", "session_a"))
            .expect("hot update");
        assert!(registry.match_domain("demo.sdkwork.link").is_none());
        assert!(registry.match_domain("demo2.sdkwork.link").is_some());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn wildcard_matches_subdomains_longest_suffix_wins() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_wild", "*.sdkwork.link", "session_a"))
            .expect("register");
        assert!(
            registry.match_domain("app.sdkwork.link").is_some(),
            "wildcard covers subdomains"
        );
        assert!(
            registry.match_domain("deep.app.sdkwork.link").is_some(),
            "wildcard covers nested subdomains"
        );
        assert!(
            registry.match_domain("sdkwork.link").is_none(),
            "wildcard does not cover the bare suffix"
        );
        assert!(registry.match_domain("app.other.link").is_none());

        // A more specific wildcard wins over a broader one; an exact route
        // wins over both.
        registry
            .register(sample_route("route_wild2", "*.app.sdkwork.link", "session_a"))
            .expect("register nested wildcard");
        let matched = registry.match_domain("x.app.sdkwork.link").expect("match");
        assert_eq!(matched.route.id.to_string(), "route_wild2");
        registry
            .register(sample_route("route_exact", "y.app.sdkwork.link", "session_a"))
            .expect("register exact");
        let matched = registry.match_domain("y.app.sdkwork.link").expect("match");
        assert_eq!(matched.route.id.to_string(), "route_exact");
        assert_eq!(registry.count(), 3);
    }

    #[test]
    fn conflicting_wildcard_is_rejected_and_teardown_sweeps() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_wild", "*.sdkwork.link", "session_a"))
            .expect("register");
        let error = registry
            .register(sample_route("route_other", "*.sdkwork.link", "session_b"))
            .expect_err("cross-session wildcard conflict");
        assert!(matches!(error, TunnelError::RouteConflict(_)));

        // Same-session hot update replaces the wildcard entry.
        registry
            .register(sample_route("route_wild", "*.renewed.link", "session_a"))
            .expect("hot update");
        assert!(registry.match_domain("x.sdkwork.link").is_none());
        assert!(registry.match_domain("x.renewed.link").is_some());

        // Session teardown returns every removed id (wildcard included).
        let removed = registry
            .remove_session(&SessionId::parse("session_a").expect("valid id"));
        assert_eq!(removed.len(), 1);
        assert!(registry.match_domain("x.renewed.link").is_none());
    }

    #[test]
    fn teardown_returns_port_route_ids() {
        let registry = RouteRegistry::new();
        let route = TunnelRoute::new(
            RouteId::parse("route_ssh").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_session(SessionId::parse("session_a").expect("valid id"), chrono::Utc::now());
        registry.register(route).expect("register");
        let removed = registry
            .remove_session(&SessionId::parse("session_a").expect("valid id"));
        assert_eq!(removed.len(), 1, "port routes must be reported to teardown");
    }

    #[test]
    fn port_routes_register_and_release() {
        let registry = RouteRegistry::new();
        let route = TunnelRoute::new(
            RouteId::parse("route_ssh").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_session(SessionId::parse("session_a").expect("valid id"), Utc::now());
        registry.register(route).expect("register");
        assert!(registry.match_port(7022).is_some());
        registry.remove_session(&SessionId::parse("session_a").expect("valid id"));
        assert!(registry.match_port(7022).is_none());
    }
}
