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

/// Upper bound on routes one session may keep registered. A worker only ever
/// needs a handful of public surfaces; the cap turns "a session leaks route
/// registrations" from unbounded registry growth into a refused request.
pub const MAX_ROUTES_PER_SESSION: usize = 256;

/// One registered route with its owning session.
#[derive(Debug, Clone)]
pub struct RegisteredRoute {
    /// The validated route (carries `session_id` ownership).
    pub route: TunnelRoute,
    /// The route's bandwidth ceiling, shared by every stream it relays so the
    /// limit bounds the service rather than each connection (FRP
    /// `transport.bandwidthLimit` parity). `None` is unlimited. Crate-internal:
    /// dispatch is the only consumer, and the limiter is an implementation
    /// detail rather than part of the registry's public data.
    pub(crate) limiter: Option<Arc<super::rate_limit::RouteRateLimiter>>,
    /// Latest probe verdict from the owning agent.
    pub health: Arc<super::health::RouteHealth>,
}

impl RegisteredRoute {
    /// The owning session id.
    pub fn session(&self) -> Option<&SessionId> {
        self.route.session_id.as_ref()
    }
}

/// Outcome of asking the registry which route serves a request.
#[derive(Debug)]
pub enum RouteSelection {
    /// This route serves the request.
    Selected(Arc<RegisteredRoute>),
    /// Routes claim the request, but every one of them is unhealthy: the
    /// caller must answer "unavailable", not "not found" (FRP E2/E3 parity).
    NoHealthyReplica,
    /// Nothing claims the request at all.
    NoRoute,
}

/// Thread-safe registry of live tunnel routes.
#[derive(Debug, Default)]
pub struct RouteRegistry {
    inner: RwLock<RegistryInner>,
    version: AtomicU64,
    /// Round-robin cursor for load-balancer groups: each selection advances it
    /// so replicas of one service share traffic evenly regardless of which
    /// matcher family they arrived through.
    selection_cursor: AtomicU64,
}

#[derive(Debug, Default)]
struct RegistryInner {
    /// Id-keyed index: O(1) lookup for `get`, and the ownership anchor that
    /// makes replace/remove decisions O(1) instead of three linear scans.
    by_id: HashMap<RouteId, Arc<RegisteredRoute>>,
    /// Exact-domain routes. A host may carry several routes when their
    /// location prefixes are disjoint (FRP `locations` parity): `/api` and
    /// `/static` are two independent claims on one hostname, and a request is
    /// dispatched to the longest prefix that covers it.
    by_domain: HashMap<String, Vec<Arc<RegisteredRoute>>>,
    /// Wildcard domain routes keyed by their suffix (`*.a.b` is stored under
    /// `a.b`); members follow the same location rules as exact hosts.
    by_wildcard: HashMap<String, Vec<Arc<RegisteredRoute>>>,
    /// Port-keyed index: TCP and UDP port namespaces are independent, so the
    /// same port number may legitimately host one route of each protocol. A
    /// port carries several routes when they share a load-balancer group
    /// (FRP `loadBalancer.group` parity): the listener is bound once, and each
    /// accepted connection is dispatched to one member.
    by_port: HashMap<(bool, u16), Vec<Arc<RegisteredRoute>>>,
    /// Secret-route index keyed by the name a visitor asks for (FRP
    /// `stcp`/`sudp` `serverName`). Nothing in this index is reachable without
    /// the matching key, which lives on the route — never here.
    by_secret: HashMap<String, Vec<Arc<RegisteredRoute>>>,
    /// Live route count per owning session — O(1) budget checks on register.
    routes_per_session: HashMap<SessionId, usize>,
}

/// True when two routes may share one public surface.
///
/// They may not, unless they are replicas of one service: the same non-empty
/// load-balancer group on both sides (FRP `loadBalancer.group` parity). Two
/// unrelated devices must still never claim one surface, which is what makes
/// the tunnel's ownership model safe.
fn replicas_of_one_service(left: &TunnelRoute, right: &TunnelRoute) -> bool {
    match (
        left.load_balancer_group.as_deref(),
        right.load_balancer_group.as_deref(),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// True when two location sets can both cover one request path.
///
/// An empty set means "every path on this host", so it overlaps everything.
/// Otherwise one prefix must cover the other, segment-aware — the same rule
/// dispatch uses, so "conflict" and "ambiguous dispatch" can never disagree.
fn locations_overlap(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return true;
    }
    left.iter().any(|a| {
        right.iter().any(|b| {
            let a = a.trim_end_matches('/');
            let b = b.trim_end_matches('/');
            a == b
                || a.strip_prefix(b).is_some_and(|rest| rest.starts_with('/'))
                || b.strip_prefix(a).is_some_and(|rest| rest.starts_with('/'))
        })
    })
}

/// Every route on one host whose location prefix covers `path`, most specific
/// first (so the caller's first candidate is the longest prefix).
fn candidates_by_path(routes: &[Arc<RegisteredRoute>], path: &str) -> Vec<Arc<RegisteredRoute>> {
    let mut matched: Vec<(usize, Arc<RegisteredRoute>)> = routes
        .iter()
        .filter_map(|route| {
            route
                .route
                .matches_path(path)
                .map(|len| (len, route.clone()))
        })
        .collect();
    matched.sort_by(|left, right| right.0.cmp(&left.0));
    matched.into_iter().map(|(_, route)| route).collect()
}

/// The route that owns a host as a whole, falling back to the first claim.
///
/// Used for ownership questions ("is this host claimed by the tunnel?"), not
/// for dispatch: dispatch must ask [`RouteRegistry::match_domain_path`].
fn pick_by_ownership(routes: &[Arc<RegisteredRoute>]) -> Option<Arc<RegisteredRoute>> {
    routes
        .iter()
        .find(|route| route.route.locations.is_empty())
        .or_else(|| routes.first())
        .cloned()
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
                if let Some(conflict) = existing.iter().find(|entry| {
                    entry.session() != Some(&owner)
                        && locations_overlap(&entry.route.locations, &route.locations)
                        && !replicas_of_one_service(&entry.route, &route)
                }) {
                    return Err(TunnelError::RouteConflict(format!(
                        "domain {domain} is already served by another device ({} overlaps)",
                        describe_locations(&conflict.route.locations)
                    )));
                }
            }
        }
        if let Some(suffix) = wildcard_key(&route) {
            if let Some(existing) = inner.by_wildcard.get(&suffix) {
                if let Some(conflict) = existing.iter().find(|entry| {
                    entry.session() != Some(&owner)
                        && locations_overlap(&entry.route.locations, &route.locations)
                        && !replicas_of_one_service(&entry.route, &route)
                }) {
                    return Err(TunnelError::RouteConflict(format!(
                        "wildcard domain *.{suffix} is already served by another device ({} overlaps)",
                        describe_locations(&conflict.route.locations)
                    )));
                }
            }
        }
        let datagram = is_datagram(&route);
        if let Some(name) = secret_key(&route) {
            if let Some(existing) = inner.by_secret.get(name) {
                if let Some(conflict) = existing.iter().find(|entry| {
                    entry.session() != Some(&owner)
                        && !replicas_of_one_service(&entry.route, &route)
                }) {
                    return Err(TunnelError::RouteConflict(format!(
                        "secret route name {name} is already served by another device (route {})",
                        conflict.route.id
                    )));
                }
            }
        }
        if let Some(port) = port_key(&route) {
            let key = (datagram, port);
            if let Some(existing) = inner.by_port.get(&key) {
                if let Some(conflict) = existing.iter().find(|entry| {
                    entry.session() != Some(&owner)
                        && !replicas_of_one_service(&entry.route, &route)
                }) {
                    return Err(TunnelError::RouteConflict(format!(
                        "gateway port {port} is already served by another device (route {})",
                        conflict.route.id
                    )));
                }
            }
        }
        // A route id belongs to whoever registered it first: a different
        // session may not take a surface over by reusing the id, even with a
        // matcher that collides with nothing.
        let replaced = inner.by_id.get(&route.id).cloned();
        if let Some(existing) = &replaced {
            if existing.session() != Some(&owner) {
                return Err(TunnelError::RouteConflict(format!(
                    "route {} is already registered by another device",
                    route.id
                )));
            }
        }
        if replaced.is_none()
            && inner.routes_per_session.get(&owner).copied().unwrap_or(0) >= MAX_ROUTES_PER_SESSION
        {
            return Err(TunnelError::InvalidRoute(format!(
                "session route budget exhausted ({MAX_ROUTES_PER_SESSION} live routes)"
            )));
        }
        if let Some(existing) = replaced {
            if let Some(domain) = domain_key(&existing.route) {
                remove_from_host_index(
                    &mut inner.by_domain,
                    &domain.to_owned(),
                    &existing.route.id,
                );
            }
            if let Some(suffix) = wildcard_key(&existing.route) {
                remove_from_host_index(&mut inner.by_wildcard, &suffix, &existing.route.id);
            }
            if let Some(port) = port_key(&existing.route) {
                remove_from_host_index(
                    &mut inner.by_port,
                    &(is_datagram(&existing.route), port),
                    &existing.route.id,
                );
            }
            if let Some(name) = secret_key(&existing.route) {
                remove_from_host_index(&mut inner.by_secret, &name.to_owned(), &existing.route.id);
            }
            decrement_session_count(&mut inner.routes_per_session, existing.session());
        }
        let registered = Arc::new(RegisteredRoute {
            limiter: route
                .bandwidth_limit_bytes_per_second
                .map(super::rate_limit::RouteRateLimiter::new)
                .map(Arc::new),
            health: Arc::new(super::health::RouteHealth::default()),
            route,
        });
        inner
            .by_id
            .insert(registered.route.id.clone(), registered.clone());
        if let Some(suffix) = wildcard_key(&registered.route) {
            inner
                .by_wildcard
                .entry(suffix.to_owned())
                .or_default()
                .push(registered.clone());
        } else if let Some(domain) = domain_key(&registered.route) {
            inner
                .by_domain
                .entry(domain.to_owned())
                .or_default()
                .push(registered.clone());
        }
        if let Some(port) = port_key(&registered.route) {
            let key = (datagram, port);
            inner
                .by_port
                .entry(key)
                .or_default()
                .push(registered.clone());
        }
        if let Some(name) = secret_key(&registered.route) {
            inner
                .by_secret
                .entry(name.to_owned())
                .or_default()
                .push(registered.clone());
        }
        *inner.routes_per_session.entry(owner).or_insert(0) += 1;
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
        let ids: Vec<RouteId> = inner
            .by_id
            .values()
            .filter(|route| route.session() == Some(session))
            .map(|route| route.route.id.clone())
            .collect();
        for id in &ids {
            remove_by_id(&mut inner, id);
        }
        if !ids.is_empty() {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        ids
    }

    /// Looks up one registered route by id.
    pub fn get(&self, id: &RouteId) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner.by_id.get(id).cloned()
    }

    /// Matches a visitor host (lowercase, port-less) to a live route, for
    /// questions about *ownership* ("does the tunnel claim this host?").
    ///
    /// Dispatch must use [`Self::match_domain_path`]: a host may carry several
    /// routes with disjoint location prefixes, and only the request path says
    /// which one serves it. Exact registrations win over wildcards; among
    /// wildcards the longest suffix wins. A wildcard covers only subdomains
    /// (`*.a.b` does not match `a.b` itself).
    pub fn match_domain(&self, host: &str) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        if let Some(exact) = inner
            .by_domain
            .get(host)
            .and_then(|routes| pick_by_ownership(routes))
        {
            return Some(exact);
        }
        // Longest suffix wins; the cover test builds no per-request string, so
        // the scan stays cheap under the read lock even with many wildcards.
        inner
            .by_wildcard
            .iter()
            .filter(|(suffix, _)| host_covers_wildcard_suffix(host, suffix))
            .max_by_key(|(suffix, _)| suffix.len())
            .and_then(|(_, routes)| pick_by_ownership(routes))
    }

    /// Matches a visitor host **and request path** to the route that serves it
    /// (FRP `locations` parity).
    ///
    /// Within one host the longest location prefix that covers `path` wins; a
    /// host whose routes all decline the path falls through to the wildcard
    /// registrations, which may still claim it. `None` means the tunnel does
    /// not serve this request at all, and the caller keeps its own routing.
    ///
    /// This answers *ownership*, so it ignores health: dispatch asks
    /// [`Self::select_domain_path`], which skips unhealthy replicas.
    pub fn match_domain_path(&self, host: &str, path: &str) -> Option<Arc<RegisteredRoute>> {
        self.candidates_domain_path(host, path).into_iter().next()
    }

    /// Every route that could serve this host and path, most specific first.
    ///
    /// Several routes appear when they form a load-balancer group (FRP
    /// `loadBalancer.group` parity): all of them are replicas of one service,
    /// and the caller picks one.
    pub fn candidates_domain_path(&self, host: &str, path: &str) -> Vec<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        let exact = inner
            .by_domain
            .get(host)
            .map(|routes| candidates_by_path(routes, path))
            .unwrap_or_default();
        if !exact.is_empty() {
            return exact;
        }
        // Longest suffix wins; the cover test builds no per-request string, so
        // the scan stays cheap under the read lock even with many wildcards.
        inner
            .by_wildcard
            .iter()
            .filter(|(suffix, _)| host_covers_wildcard_suffix(host, suffix))
            .max_by_key(|(suffix, _)| suffix.len())
            .map(|(_, routes)| candidates_by_path(routes, path))
            .unwrap_or_default()
    }

    /// Picks the route that serves this host and path, skipping replicas the
    /// agent has reported unhealthy (FRP `healthCheck` + `loadBalancer`).
    ///
    /// Healthy members of the chosen group are rotated round-robin, so two
    /// replicas of one service share traffic instead of the first one taking
    /// everything.
    pub fn select_domain_path(&self, host: &str, path: &str) -> RouteSelection {
        let candidates = self.candidates_domain_path(host, path);
        self.select_from(candidates)
    }

    /// Matches a gateway TCP listener port to a live route (ownership view).
    pub fn match_port(&self, port: u16) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner_by_port(&inner, false, port).into_iter().next()
    }

    /// Picks the route that serves one accepted TCP connection on `port`,
    /// skipping unhealthy group members.
    pub fn select_port(&self, port: u16) -> RouteSelection {
        let candidates = {
            let inner = self
                .inner
                .read()
                .expect("route registry lock is never held across awaits");
            inner_by_port(&inner, false, port)
        };
        self.select_from(candidates)
    }

    /// Applies the selection policy to a candidate set.
    fn select_from(&self, candidates: Vec<Arc<RegisteredRoute>>) -> RouteSelection {
        if candidates.is_empty() {
            return RouteSelection::NoRoute;
        }
        // A single candidate is the common case and stays allocation-free: no
        // rotation, no filtering beyond its own health flag.
        if candidates.len() == 1 {
            let only = candidates.into_iter().next().expect("length was checked");
            return if only.health.is_healthy() {
                RouteSelection::Selected(only)
            } else {
                RouteSelection::NoHealthyReplica
            };
        }
        let healthy: Vec<Arc<RegisteredRoute>> = candidates
            .into_iter()
            .filter(|route| route.health.is_healthy())
            .collect();
        if healthy.is_empty() {
            return RouteSelection::NoHealthyReplica;
        }
        let cursor = self.selection_cursor.fetch_add(1, Ordering::Relaxed);
        let index = (cursor % healthy.len() as u64) as usize;
        RouteSelection::Selected(healthy[index].clone())
    }

    /// Every route registered under a secret name, healthy or not (FRP
    /// `stcp`/`sudp` `serverName` lookup).
    pub fn candidates_secret(&self, name: &str) -> Vec<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner.by_secret.get(name).cloned().unwrap_or_default()
    }

    /// Selects the route that serves a secret request, skipping unhealthy
    /// replicas exactly like the public planes.
    ///
    /// The caller must still verify the visitor's key against the returned
    /// route: selection is about *which* replica, never about *whether* the
    /// caller is allowed.
    pub fn select_secret(&self, name: &str) -> RouteSelection {
        self.select_from(self.candidates_secret(name))
    }

    /// Matches a gateway UDP listener port to a live route.    ///
    /// Datagram ports are single-owner (a group is refused for UDP routes at
    /// validation), so there is nothing to select between.
    pub fn match_udp_port(&self, port: u16) -> Option<Arc<RegisteredRoute>> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner_by_port(&inner, true, port).into_iter().next()
    }

    /// Every registered route, sorted by id for stable API output.
    pub fn list(&self) -> Vec<TunnelRoute> {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        let mut routes: Vec<TunnelRoute> = inner
            .by_id
            .values()
            .map(|route| route.route.clone())
            .collect();
        routes.sort_by(|left, right| left.id.cmp(&right.id));
        routes
    }

    /// Number of registered routes.
    pub fn count(&self) -> usize {
        let inner = self
            .inner
            .read()
            .expect("route registry lock is never held across awaits");
        inner.by_id.len()
    }
}

fn domain_key(route: &TunnelRoute) -> Option<&str> {
    route
        .matcher
        .as_domain()
        .filter(|domain| !domain.starts_with("*."))
}

/// Drops one route from an index, removing the key entirely when its last
/// route leaves (so "is this surface claimed" stays cheap).
fn remove_from_host_index<K: std::hash::Hash + Eq>(
    index: &mut HashMap<K, Vec<Arc<RegisteredRoute>>>,
    key: &K,
    id: &RouteId,
) {
    let Some(routes) = index.get_mut(key) else {
        return;
    };
    routes.retain(|route| &route.route.id != id);
    if routes.is_empty() {
        index.remove(key);
    }
}

/// Human-readable location set for conflict messages.
fn describe_locations(locations: &[String]) -> String {
    if locations.is_empty() {
        "every path".to_owned()
    } else {
        locations.join(", ")
    }
}

fn wildcard_key(route: &TunnelRoute) -> Option<String> {
    route.matcher.wildcard_suffix().map(str::to_owned)
}

fn is_datagram(route: &TunnelRoute) -> bool {
    route.protocol.is_datagram()
}

/// The secret name a route answers to, when it is a secret route.
fn secret_key(route: &TunnelRoute) -> Option<&str> {
    route.matcher.as_secret()
}

fn inner_by_port(inner: &RegistryInner, datagram: bool, port: u16) -> Vec<Arc<RegisteredRoute>> {
    inner
        .by_port
        .get(&(datagram, port))
        .cloned()
        .unwrap_or_default()
}

fn port_key(route: &TunnelRoute) -> Option<u16> {
    route.matcher.as_port()
}

/// Whether `*.{suffix}` covers `host` (the host ends with the suffix preceded
/// by a dot), computed without building the joined string — this runs for
/// every wildcard on every relayed request.
fn host_covers_wildcard_suffix(host: &str, suffix: &str) -> bool {
    let host = host.as_bytes();
    let suffix = suffix.as_bytes();
    host.len() > suffix.len() + 1
        && host[host.len() - suffix.len() - 1] == b'.'
        && host.ends_with(suffix)
}

fn decrement_session_count(counts: &mut HashMap<SessionId, usize>, session: Option<&SessionId>) {
    if let Some(session) = session {
        if let Some(count) = counts.get_mut(session) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(session);
            }
        }
    }
}

fn remove_by_id(inner: &mut RegistryInner, id: &RouteId) -> Option<Arc<RegisteredRoute>> {
    let registered = inner.by_id.remove(id)?;
    if let Some(domain) = domain_key(&registered.route) {
        remove_from_host_index(&mut inner.by_domain, &domain.to_owned(), id);
    }
    if let Some(suffix) = wildcard_key(&registered.route) {
        remove_from_host_index(&mut inner.by_wildcard, &suffix, id);
    }
    if let Some(port) = port_key(&registered.route) {
        let key = (is_datagram(&registered.route), port);
        remove_from_host_index(&mut inner.by_port, &key, id);
    }
    if let Some(name) = secret_key(&registered.route) {
        remove_from_host_index(&mut inner.by_secret, &name.to_owned(), id);
    }
    decrement_session_count(&mut inner.routes_per_session, registered.session());
    Some(registered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sdkwork_webserver_tunnel_core::{
        local_target, local_udp_target, RouteMatcher, RoutePolicy, TunnelProtocolKind,
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

    /// A secret route (FRP `stcp`) owned by `session`, keyed by `key`.
    fn secret_route(
        id: &str,
        name: &str,
        session: &str,
        key: &str,
        group: Option<&str>,
    ) -> TunnelRoute {
        TunnelRoute::new(
            RouteId::parse(id).expect("valid id"),
            name,
            TunnelProtocolKind::Stcp,
            RouteMatcher::secret(name).expect("valid secret name"),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_secret_key(Some(key.to_owned()))
        .expect("key attaches")
        .with_load_balancing(group.map(str::to_owned), None)
        .expect("group attaches")
        .with_session(SessionId::parse(session).expect("valid id"), Utc::now())
    }

    #[test]
    fn a_secret_name_belongs_to_one_device_unless_it_is_a_group() {
        let registry = RouteRegistry::new();
        let first = registry
            .register(secret_route(
                "route_ssh_a",
                "private-ssh",
                "session_a",
                "key-one-123",
                None,
            ))
            .expect("the first claim registers");

        // The name is the surface: a second device may not serve it without
        // joining the group, exactly like a hostname or a port.
        let conflict = registry
            .register(secret_route(
                "route_ssh_b",
                "private-ssh",
                "session_b",
                "key-two-123",
                None,
            ))
            .expect_err("a second ungrouped claim must conflict");
        assert!(matches!(conflict, TunnelError::RouteConflict(_)));

        match registry.select_secret("private-ssh") {
            RouteSelection::Selected(route) => {
                assert_eq!(route.route.id, first.route.id);
                assert!(
                    route.route.secret_key_matches("key-one-123"),
                    "the registered key is the one the gateway verifies"
                );
                assert!(!route.route.secret_key_matches("key-two-123"));
            }
            other => panic!("expected the registered route, got {other:?}"),
        }
        assert!(matches!(
            registry.select_secret("no-such-route"),
            RouteSelection::NoRoute
        ));

        // Releasing the name lets another device take it, so a provider that
        // restarted is not locked out by its own stale registration.
        registry.unregister(&first.route.id).expect("removes");
        assert!(matches!(
            registry.select_secret("private-ssh"),
            RouteSelection::NoRoute
        ));
        registry
            .register(secret_route(
                "route_ssh_c",
                "private-ssh",
                "session_c",
                "key-three-123",
                None,
            ))
            .expect("the name is free again");
    }

    #[test]
    fn secret_replicas_share_a_name_only_inside_their_group() {
        let registry = RouteRegistry::new();
        let first = registry
            .register(secret_route(
                "route_ssh_a",
                "private-ssh",
                "session_a",
                "shared-key-123",
                Some("ssh"),
            ))
            .expect("first replica");
        let second = registry
            .register(secret_route(
                "route_ssh_b",
                "private-ssh",
                "session_b",
                "shared-key-123",
                Some("ssh"),
            ))
            .expect("same-group replica");

        let mut ids = std::collections::HashSet::new();
        for _ in 0..4 {
            match registry.select_secret("private-ssh") {
                RouteSelection::Selected(route) => {
                    ids.insert(route.route.id.to_string());
                }
                other => panic!("expected a selection, got {other:?}"),
            }
        }
        assert_eq!(ids.len(), 2, "both replicas serve, got {ids:?}");

        // Health steering applies to secret routes too: a visitor must not be
        // sent to a replica that reported itself down.
        first.health.apply_probe(false, 3);
        for _ in 0..4 {
            match registry.select_secret("private-ssh") {
                RouteSelection::Selected(route) => assert_eq!(route.route.id, second.route.id),
                other => panic!("expected the healthy replica, got {other:?}"),
            }
        }
        second.health.apply_probe(false, 1);
        assert!(matches!(
            registry.select_secret("private-ssh"),
            RouteSelection::NoHealthyReplica
        ));
    }

    fn sample_port_route(id: &str, port: u16, session: &str) -> TunnelRoute {
        TunnelRoute::new(
            RouteId::parse(id).expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(port),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_session(SessionId::parse(session).expect("valid id"), Utc::now())
    }

    fn sample_udp_route(id: &str, port: u16, session: &str) -> TunnelRoute {
        TunnelRoute::new(
            RouteId::parse(id).expect("valid id"),
            "dns",
            TunnelProtocolKind::Udp,
            RouteMatcher::Port(port),
            local_udp_target(53),
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

    /// A route on `domain` that answers only under `locations`.
    fn located_route(id: &str, domain: &str, session: &str, locations: &[&str]) -> TunnelRoute {
        sample_route(id, domain, session)
            .with_locations(locations.iter().map(|value| (*value).to_owned()).collect())
            .expect("locations attach")
    }

    #[test]
    fn disjoint_location_claims_share_one_host_and_dispatch_by_longest_prefix() {
        // FRP `locations` parity: `/api` and `/static` are two independent
        // claims on one hostname, and the request path decides between them.
        let registry = RouteRegistry::new();
        registry
            .register(located_route(
                "route_api",
                "demo.sdkwork.link",
                "session_a",
                &["/api", "/api/v2"],
            ))
            .expect("first claim registers");
        registry
            .register(located_route(
                "route_static",
                "demo.sdkwork.link",
                "session_b",
                &["/static"],
            ))
            .expect("a disjoint claim on the same host registers");

        let api = registry
            .match_domain_path("demo.sdkwork.link", "/api/users")
            .expect("the /api claim serves it");
        assert_eq!(api.route.id.to_string(), "route_api");
        let v2 = registry
            .match_domain_path("demo.sdkwork.link", "/api/v2/users")
            .expect("the longer prefix wins");
        assert_eq!(v2.route.id.to_string(), "route_api");
        let statics = registry
            .match_domain_path("demo.sdkwork.link", "/static/app.js")
            .expect("the other claim serves its own prefix");
        assert_eq!(statics.route.id.to_string(), "route_static");
        assert!(
            registry
                .match_domain_path("demo.sdkwork.link", "/other")
                .is_none(),
            "a path nobody claimed must fall through to local routing"
        );
        // Ownership questions still see the host as claimed.
        assert!(registry.match_domain("demo.sdkwork.link").is_some());
    }

    #[test]
    fn an_overlapping_location_claim_is_a_conflict() {
        let registry = RouteRegistry::new();
        registry
            .register(located_route(
                "route_api",
                "demo.sdkwork.link",
                "session_a",
                &["/api"],
            ))
            .expect("register");
        // `/api` vs `/api/v2` overlap: both would answer `/api/v2/x`.
        let nested = registry
            .register(located_route(
                "route_v2",
                "demo.sdkwork.link",
                "session_b",
                &["/api/v2"],
            ))
            .expect_err("an overlapping prefix is ambiguous");
        assert!(matches!(nested, TunnelError::RouteConflict(_)));

        // A whole-host claim overlaps everything on that host.
        let whole = registry
            .register(sample_route("route_all", "demo.sdkwork.link", "session_b"))
            .expect_err("a whole-host claim overlaps the /api claim");
        assert!(matches!(whole, TunnelError::RouteConflict(_)));

        // Disjoint prefixes are fine, in either direction.
        registry
            .register(located_route(
                "route_other",
                "demo.sdkwork.link",
                "session_b",
                &["/other"],
            ))
            .expect("a disjoint prefix registers");

        // Removing the host's last claim frees the host for a whole-host one.
        registry.unregister(&RouteId::parse("route_api").expect("valid id"));
        registry.unregister(&RouteId::parse("route_other").expect("valid id"));
        registry
            .register(sample_route("route_all", "demo.sdkwork.link", "session_b"))
            .expect("the host is free again");
    }

    /// A route on `domain` that belongs to load-balancer group `group`.
    fn grouped_route(id: &str, domain: &str, session: &str, group: &str) -> TunnelRoute {
        sample_route(id, domain, session)
            .with_load_balancing(Some(group.to_owned()), None)
            .expect("group attaches")
    }

    #[test]
    fn replicas_of_one_group_share_a_surface_and_rotate() {
        // FRP `loadBalancer.group` parity: two devices may serve one hostname
        // when they declare the same group, and traffic rotates between them.
        let registry = RouteRegistry::new();
        registry
            .register(grouped_route(
                "route_a",
                "lb.sdkwork.link",
                "session_a",
                "web",
            ))
            .expect("first replica registers");
        registry
            .register(grouped_route(
                "route_b",
                "lb.sdkwork.link",
                "session_b",
                "web",
            ))
            .expect("a same-group replica registers on the same host");

        let mut picked: Vec<String> = Vec::new();
        for _ in 0..4 {
            match registry.select_domain_path("lb.sdkwork.link", "/") {
                RouteSelection::Selected(route) => picked.push(route.route.id.to_string()),
                other => panic!("expected a selection, got {other:?}"),
            }
        }
        picked.sort();
        picked.dedup();
        assert_eq!(
            picked.len(),
            2,
            "both replicas must serve traffic, got {picked:?}"
        );

        // A different group is still a conflict: grouping is the *only* way
        // two devices may share one surface.
        let conflict = registry
            .register(grouped_route(
                "route_c",
                "lb.sdkwork.link",
                "session_c",
                "api",
            ))
            .expect_err("another group must not take the same host");
        assert!(matches!(conflict, TunnelError::RouteConflict(_)));

        // A route with no group at all is a conflict too.
        let ungrouped = registry
            .register(sample_route("route_d", "lb.sdkwork.link", "session_d"))
            .expect_err("an ungrouped route must not take a grouped host");
        assert!(matches!(ungrouped, TunnelError::RouteConflict(_)));
    }

    #[test]
    fn an_unhealthy_replica_is_skipped_and_all_down_is_reported() {
        let registry = RouteRegistry::new();
        let first = registry
            .register(grouped_route(
                "route_a",
                "lb.sdkwork.link",
                "session_a",
                "web",
            ))
            .expect("first replica");
        let second = registry
            .register(grouped_route(
                "route_b",
                "lb.sdkwork.link",
                "session_b",
                "web",
            ))
            .expect("second replica");

        // The agent behind the first replica reports it down.
        first.health.apply_probe(false, 3);
        for _ in 0..4 {
            match registry.select_domain_path("lb.sdkwork.link", "/") {
                RouteSelection::Selected(route) => assert_eq!(
                    route.route.id.to_string(),
                    "route_b",
                    "traffic must avoid the unhealthy replica"
                ),
                other => panic!("expected the healthy replica, got {other:?}"),
            }
        }

        // Both down: the caller must answer "unavailable", not "not found".
        second.health.apply_probe(false, 1);
        assert!(matches!(
            registry.select_domain_path("lb.sdkwork.link", "/"),
            RouteSelection::NoHealthyReplica
        ));
        // The host is still *claimed*, so the ownership view is unchanged.
        assert!(registry.match_domain_path("lb.sdkwork.link", "/").is_some());

        // Recovery is immediate on the next successful probe.
        first.health.apply_probe(true, 0);
        assert!(matches!(
            registry.select_domain_path("lb.sdkwork.link", "/"),
            RouteSelection::Selected(_)
        ));
    }

    #[test]
    fn a_group_also_shares_a_gateway_port() {
        let registry = RouteRegistry::new();
        let mut first = sample_route("route_a", "a.sdkwork.link", "session_a");
        first.protocol = sdkwork_webserver_tunnel_core::TunnelProtocolKind::Tcp;
        first.matcher = RouteMatcher::Port(7022);
        first.load_balancer_group = Some("ssh".to_owned());
        let first = registry.register(first).expect("first port replica");

        let mut second = sample_route("route_b", "b.sdkwork.link", "session_b");
        second.protocol = sdkwork_webserver_tunnel_core::TunnelProtocolKind::Tcp;
        second.matcher = RouteMatcher::Port(7022);
        second.load_balancer_group = Some("ssh".to_owned());
        let second = registry.register(second).expect("same-group port replica");

        // Both are candidates for one accepted connection; the listener is
        // bound once (the gateway control loop checks `is_served`).
        let mut ids = std::collections::HashSet::new();
        for _ in 0..4 {
            match registry.select_port(7022) {
                RouteSelection::Selected(route) => {
                    ids.insert(route.route.id.to_string());
                }
                other => panic!("expected a selection, got {other:?}"),
            }
        }
        assert_eq!(ids.len(), 2, "both port replicas serve, got {ids:?}");

        first.health.apply_probe(false, 1);
        second.health.apply_probe(false, 1);
        assert!(matches!(
            registry.select_port(7022),
            RouteSelection::NoHealthyReplica
        ));
    }

    #[test]
    fn a_host_with_only_a_declined_path_falls_through_to_the_wildcard_claim() {
        // The wildcard route owns the whole host set, so a path the exact
        // claim declines is still served by the wildcard instead of 404ing.
        let registry = RouteRegistry::new();
        registry
            .register(located_route(
                "route_api",
                "app.sdkwork.link",
                "session_a",
                &["/api"],
            ))
            .expect("exact claim registers");
        registry
            .register(sample_route("route_wild", "*.sdkwork.link", "session_b"))
            .expect("wildcard claim registers");

        let api = registry
            .match_domain_path("app.sdkwork.link", "/api/x")
            .expect("exact wins");
        assert_eq!(api.route.id.to_string(), "route_api");
        let other = registry
            .match_domain_path("app.sdkwork.link", "/other")
            .expect("wildcard covers the rest");
        assert_eq!(other.route.id.to_string(), "route_wild");
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
            .register(sample_route(
                "route_wild2",
                "*.app.sdkwork.link",
                "session_a",
            ))
            .expect("register nested wildcard");
        let matched = registry.match_domain("x.app.sdkwork.link").expect("match");
        assert_eq!(matched.route.id.to_string(), "route_wild2");
        registry
            .register(sample_route(
                "route_exact",
                "y.app.sdkwork.link",
                "session_a",
            ))
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
        let removed = registry.remove_session(&SessionId::parse("session_a").expect("valid id"));
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
        .with_session(
            SessionId::parse("session_a").expect("valid id"),
            chrono::Utc::now(),
        );
        registry.register(route).expect("register");
        let removed = registry.remove_session(&SessionId::parse("session_a").expect("valid id"));
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

    #[test]
    fn tcp_and_udp_port_namespaces_are_independent() {
        // One port number may legitimately host one route of each protocol:
        // the gateway binds a stream listener and a datagram socket, and the
        // two indexes must never hand a visitor the other plane's route.
        let registry = RouteRegistry::new();
        registry
            .register(sample_port_route("route_tcp", 7000, "session_a"))
            .expect("register tcp");
        registry
            .register(sample_udp_route("route_udp", 7000, "session_a"))
            .expect("register udp on the same port number");
        assert_eq!(registry.count(), 2, "both planes keep their own entry");

        assert_eq!(
            registry
                .match_port(7000)
                .expect("tcp match")
                .route
                .id
                .to_string(),
            "route_tcp"
        );
        assert_eq!(
            registry
                .match_udp_port(7000)
                .expect("udp match")
                .route
                .id
                .to_string(),
            "route_udp"
        );

        // Removing one plane leaves the other's claim intact.
        registry
            .unregister(&RouteId::parse("route_udp").expect("valid id"))
            .expect("udp route present");
        assert!(registry.match_port(7000).is_some());
        assert!(registry.match_udp_port(7000).is_none());
    }

    #[test]
    fn a_udp_only_port_is_invisible_to_tcp_visitors() {
        // A TCP visitor hitting a port that only hosts a datagram route must
        // find nothing: the TCP plane carries no routing key that could
        // distinguish datagram sessions.
        let registry = RouteRegistry::new();
        registry
            .register(sample_udp_route("route_dns", 7000, "session_a"))
            .expect("register udp");
        assert!(registry.match_port(7000).is_none());
        assert!(registry.match_udp_port(7000).is_some());
    }

    #[test]
    fn port_conflicts_are_per_namespace_and_per_session() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_port_route("route_tcp", 7000, "session_a"))
            .expect("register tcp");
        // Same protocol, same port, different device ⇒ conflict.
        let error = registry
            .register(sample_port_route("route_other", 7000, "session_b"))
            .expect_err("cross-session tcp port conflict");
        assert!(matches!(error, TunnelError::RouteConflict(_)));
        // Same port number on the other namespace is not a conflict (proven
        // above, re-asserted here for the cross-session variant).
        registry
            .register(sample_udp_route("route_dns", 7000, "session_b"))
            .expect("udp namespace is free");
    }

    #[test]
    fn a_route_without_an_owning_session_is_rejected() {
        // Ownership is what makes every later conflict and teardown check
        // meaningful, so a session-less route must never enter the indexes.
        let registry = RouteRegistry::new();
        let orphan = TunnelRoute::new(
            RouteId::parse("route_orphan").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("orphan.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy::private(),
        )
        .expect("valid route");
        let error = registry.register(orphan).expect_err("session is required");
        assert!(matches!(error, TunnelError::InvalidRoute(_)));
        assert_eq!(registry.count(), 0);
        assert!(registry.match_domain("orphan.sdkwork.link").is_none());
    }

    #[test]
    fn version_changes_on_every_mutation_and_not_on_a_no_op() {
        let registry = RouteRegistry::new();
        let start = registry.version();

        registry
            .register(sample_route("route_a", "demo.sdkwork.link", "session_a"))
            .expect("register");
        let after_register = registry.version();
        assert!(after_register > start, "registration is a mutation");

        registry
            .unregister(&RouteId::parse("route_a").expect("valid id"))
            .expect("present");
        let after_unregister = registry.version();
        assert!(after_unregister > after_register, "removal is a mutation");

        registry
            .register(sample_route("route_b", "b.sdkwork.link", "session_a"))
            .expect("register");
        let after_second = registry.version();
        registry.remove_session(&SessionId::parse("session_a").expect("valid id"));
        assert!(registry.version() > after_second, "teardown is a mutation");

        // A no-op must not bump the version, or every dispatcher cache would
        // be invalidated by traffic that changed nothing.
        let idle = registry.version();
        assert!(registry
            .unregister(&RouteId::parse("route_ghost").expect("valid id"))
            .is_none());
        assert!(registry
            .remove_session(&SessionId::parse("session_ghost").expect("valid id"))
            .is_empty());
        assert_eq!(registry.version(), idle);
    }

    #[test]
    fn list_covers_every_index_family_sorted_and_without_duplicates() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_c", "c.sdkwork.link", "session_a"))
            .expect("register");
        registry
            .register(sample_route("route_a", "a.sdkwork.link", "session_a"))
            .expect("register");
        registry
            .register(sample_route("route_wild", "*.wild.link", "session_a"))
            .expect("register wildcard");
        registry
            .register(sample_port_route("route_ssh", 7022, "session_a"))
            .expect("register port");
        registry
            .register(sample_udp_route("route_dns", 7053, "session_a"))
            .expect("register udp");

        let ids: Vec<String> = registry
            .list()
            .into_iter()
            .map(|route| route.id.to_string())
            .collect();
        assert_eq!(
            ids,
            vec!["route_a", "route_c", "route_dns", "route_ssh", "route_wild"],
            "list() must be a stable, duplicate-free view over every index"
        );
        assert_eq!(registry.count(), ids.len(), "count and list agree");
    }

    #[test]
    fn get_resolves_routes_from_every_index_family() {
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_wild", "*.wild.link", "session_a"))
            .expect("register wildcard");
        registry
            .register(sample_udp_route("route_dns", 7053, "session_a"))
            .expect("register udp");
        for id in ["route_wild", "route_dns"] {
            assert!(
                registry
                    .get(&RouteId::parse(id).expect("valid id"))
                    .is_some(),
                "{id} must be reachable by id"
            );
        }
        assert!(registry
            .get(&RouteId::parse("route_ghost").expect("valid id"))
            .is_none());
    }

    #[test]
    fn hot_update_moves_a_route_between_matcher_families() {
        // A same-session re-registration may change the *kind* of public
        // surface (domain → port). The old index entry must be released, or
        // the route would keep answering on a surface it no longer declares.
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_x", "demo.sdkwork.link", "session_a"))
            .expect("register domain route");
        registry
            .register(sample_port_route("route_x", 7022, "session_a"))
            .expect("hot update to a port route");

        assert!(
            registry.match_domain("demo.sdkwork.link").is_none(),
            "the vacated domain must stop resolving"
        );
        assert!(registry.match_port(7022).is_some());
        assert_eq!(registry.count(), 1, "a hot update never duplicates");
    }

    #[test]
    fn a_route_id_cannot_be_taken_over_by_another_session() {
        // The route id is owned by whoever registered it first: a different
        // session reusing the id — even with an unrelated matcher — must be
        // refused instead of silently replacing the live entry.
        let registry = RouteRegistry::new();
        registry
            .register(sample_route("route_x", "demo.sdkwork.link", "session_a"))
            .expect("register");
        let error = registry
            .register(sample_port_route("route_x", 7999, "session_b"))
            .expect_err("cross-session id reuse");
        assert!(matches!(error, TunnelError::RouteConflict(_)));
        // The original route is untouched and still answers.
        assert!(registry.match_domain("demo.sdkwork.link").is_some());
        assert!(registry.match_port(7999).is_none());
    }

    #[test]
    fn a_session_route_budget_bounds_registration_fan_out() {
        // One session leaking registrations must hit the budget instead of
        // growing the registry without bound; the next session is unaffected.
        let registry = RouteRegistry::new();
        for index in 0..MAX_ROUTES_PER_SESSION {
            registry
                .register(sample_route(
                    &format!("route_{index}"),
                    &format!("app{index}.sdkwork.link"),
                    "session_a",
                ))
                .expect("register inside the budget");
        }
        let error = registry
            .register(sample_route("route_over", "over.sdkwork.link", "session_a"))
            .expect_err("budget exhausted");
        assert!(matches!(error, TunnelError::InvalidRoute(_)));
        // A hot update of a live route is not a new registration and must
        // still succeed at the cap.
        registry
            .register(sample_route(
                "route_0",
                "renamed0.sdkwork.link",
                "session_a",
            ))
            .expect("hot update at the cap");
        // And the budget is per session.
        registry
            .register(sample_route(
                "route_other",
                "other.sdkwork.link",
                "session_b",
            ))
            .expect("the next session has its own budget");
        // Teardown releases budget.
        registry.remove_session(&SessionId::parse("session_a").expect("valid id"));
        registry
            .register(sample_route(
                "route_again",
                "again.sdkwork.link",
                "session_a",
            ))
            .expect("budget released by teardown");
    }

    #[test]
    fn the_wildcard_cover_test_demands_a_dot_label_boundary() {
        // `*.suffix` covers `<label>.suffix` only: a host that merely *ends
        // with* the suffix characters is not covered.
        assert!(host_covers_wildcard_suffix(
            "app.sdkwork.link",
            "sdkwork.link"
        ));
        assert!(host_covers_wildcard_suffix(
            "deep.app.sdkwork.link",
            "sdkwork.link"
        ));
        assert!(!host_covers_wildcard_suffix("sdkwork.link", "sdkwork.link"));
        assert!(!host_covers_wildcard_suffix(
            "notsdkwork.link",
            "sdkwork.link"
        ));
        assert!(!host_covers_wildcard_suffix("link", "sdkwork.link"));
    }
}
