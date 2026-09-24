use sdkwork_routes_webserver_backend_api::backend_route_manifest;
use sdkwork_web_core::{HttpMethod, RateLimitTier, RouteAuth};

#[test]
fn application_control_plane_routes_keep_authorization_contracts() {
    let expected = [
        (
            HttpMethod::Get,
            "applications.retrieve",
            "web.sites.read",
            false,
            None,
        ),
        (
            HttpMethod::Patch,
            "applications.update",
            "web.sites.write",
            true,
            None,
        ),
        (
            HttpMethod::Delete,
            "applications.delete",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Post,
            "applications.activate",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Post,
            "applications.pause",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Delete,
            "applications.domains.delete",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        // applications.deployments.create/.rollback were retired: the
        // deployment executor is owned by sdkwork-deployments, so this
        // surface must not accept work it can never advance.
    ];
    let manifest = backend_route_manifest();

    for (method, operation_id, permission, idempotent, rate_limit_tier) in expected {
        let route = manifest
            .routes()
            .iter()
            .find(|route| route.operation_id == operation_id)
            .unwrap_or_else(|| panic!("missing route manifest entry for {operation_id}"));

        assert_eq!(route.method, method, "method mismatch for {operation_id}");
        assert_eq!(
            route.auth,
            RouteAuth::DualToken,
            "auth mismatch for {operation_id}"
        );
        assert_eq!(
            route.required_permission,
            Some(permission),
            "permission mismatch for {operation_id}"
        );
        assert_eq!(
            route.idempotent, idempotent,
            "idempotency mismatch for {operation_id}"
        );
        assert_eq!(
            route.rate_limit_tier, rate_limit_tier,
            "rate limit mismatch for {operation_id}"
        );
    }
}

#[test]
fn domain_asset_routes_keep_authorization_and_runtime_change_contracts() {
    let expected = [
        (
            HttpMethod::Get,
            "domains.list",
            "web.sites.read",
            false,
            None,
        ),
        (
            HttpMethod::Post,
            "domains.create",
            "web.sites.write",
            true,
            None,
        ),
        (
            HttpMethod::Delete,
            "domains.delete",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Put,
            "domains.applicationBinding.update",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Delete,
            "domains.applicationBinding.delete",
            "web.sites.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
    ];
    let manifest = backend_route_manifest();

    for (method, operation_id, permission, idempotent, rate_limit_tier) in expected {
        let route = manifest
            .routes()
            .iter()
            .find(|route| route.operation_id == operation_id)
            .unwrap_or_else(|| panic!("missing route manifest entry for {operation_id}"));
        assert_eq!(route.method, method, "method mismatch for {operation_id}");
        assert_eq!(route.auth, RouteAuth::DualToken);
        assert_eq!(route.required_permission, Some(permission));
        assert_eq!(route.idempotent, idempotent);
        assert_eq!(route.rate_limit_tier, rate_limit_tier);
    }
}

#[test]
fn listener_certificate_binding_routes_keep_security_contracts() {
    let expected = [
        (
            HttpMethod::Get,
            "applications.domains.listenerCertificateBindings.list",
            "web.certificates.read",
            false,
            None,
        ),
        (
            HttpMethod::Post,
            "applications.domains.listenerCertificateBindings.create",
            "web.certificates.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
        (
            HttpMethod::Delete,
            "applications.domains.listenerCertificateBindings.delete",
            "web.certificates.write",
            true,
            Some(RateLimitTier::AuthCritical),
        ),
    ];
    let manifest = backend_route_manifest();

    for (method, operation_id, permission, idempotent, rate_limit_tier) in expected {
        let route = manifest
            .routes()
            .iter()
            .find(|route| route.operation_id == operation_id)
            .unwrap_or_else(|| panic!("missing route manifest entry for {operation_id}"));
        assert_eq!(route.method, method, "method mismatch for {operation_id}");
        assert_eq!(route.auth, RouteAuth::DualToken);
        assert_eq!(route.required_permission, Some(permission));
        assert_eq!(route.idempotent, idempotent);
        assert_eq!(route.rate_limit_tier, rate_limit_tier);
    }
}

#[test]
fn root_domain_zone_routes_keep_authorization_and_navigation_contracts() {
    let expected = [
        (HttpMethod::Get, "rootDomains.list", "web.sites.read", false),
        (
            HttpMethod::Post,
            "rootDomains.create",
            "web.sites.write",
            true,
        ),
        (
            HttpMethod::Get,
            "rootDomains.retrieve",
            "web.sites.read",
            false,
        ),
        (
            HttpMethod::Delete,
            "rootDomains.delete",
            "web.sites.write",
            true,
        ),
        (
            HttpMethod::Patch,
            "rootDomains.update",
            "web.sites.write",
            true,
        ),
        (
            HttpMethod::Get,
            "rootDomains.subdomains.list",
            "web.sites.read",
            false,
        ),
        (
            HttpMethod::Post,
            "rootDomains.subdomains.create",
            "web.sites.write",
            true,
        ),
    ];
    let manifest = backend_route_manifest();

    for (method, operation_id, permission, idempotent) in expected {
        let route = manifest
            .routes()
            .iter()
            .find(|route| route.operation_id == operation_id)
            .unwrap_or_else(|| panic!("missing route manifest entry for {operation_id}"));
        assert_eq!(route.method, method, "method mismatch for {operation_id}");
        assert_eq!(route.auth, RouteAuth::DualToken);
        assert_eq!(route.required_permission, Some(permission));
        assert_eq!(route.idempotent, idempotent);
    }
}

/// The cluster plane is host-scoped (PRD-FR-030), so it is gated twice: the
/// route carries a `web.cluster.*` permission *and* the handler requires the
/// platform operator tenant.
///
/// Both halves are load-bearing. The tenant check alone would make the surface
/// reachable by every tenant member inside the operator tenant; the permission
/// check alone would let any tenant holding `web.cluster.*` reach another
/// tenant's hosts. This locks the permission half in place, because the tenant
/// half is covered by the handler tests in `cluster_routes` and
/// `cluster_ops`.
#[test]
fn cluster_plane_routes_stay_behind_web_cluster_permissions() {
    let expected = [
        (HttpMethod::Get, "clusters.list", "web.cluster.read", false),
        (
            HttpMethod::Post,
            "clusters.create",
            "web.cluster.write",
            true,
        ),
        (
            HttpMethod::Get,
            "clusters.hosts.list",
            "web.cluster.read",
            false,
        ),
        (
            HttpMethod::Get,
            "clusters.instances.list",
            "web.cluster.read",
            false,
        ),
    ];
    let manifest = backend_route_manifest();

    for (method, operation_id, permission, idempotent) in expected {
        let route = manifest
            .routes()
            .iter()
            .find(|route| route.operation_id == operation_id)
            .unwrap_or_else(|| panic!("missing route manifest entry for {operation_id}"));
        assert_eq!(route.method, method, "method mismatch for {operation_id}");
        assert_eq!(
            route.required_permission,
            Some(permission),
            "the platform operator tenant must not become a permission bypass for {operation_id}"
        );
        assert_eq!(route.idempotent, idempotent);
    }
}
