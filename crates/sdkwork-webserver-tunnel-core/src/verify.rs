//! Offline configuration preflight (FRP `frpc verify` / `frps verify` parity).
//!
//! Validation answers "is this route well formed?". Preflight answers the
//! questions an operator actually has *before* starting a process: will the
//! secret key resolve, will two routes fight over one surface, will the health
//! probe ever pass, will the load-balancer group ever be selected — and it
//! answers them **without a gateway**, because the failures that matter most
//! are the ones a connection would mask until the first visitor arrives.
//!
//! Every check returns a [`Finding`] rather than a bare error: a report that
//! lists what is wrong per route is what makes a preflight usable, and a
//! caller can decide that warnings are acceptable while errors are not
//! (`config` refuses to start on warnings too, because a warning here means
//! "this will not behave the way the configuration reads").

use crate::config::{TunnelConfig, TunnelRouteTemplate};
use crate::route::{RouteMatcher, TunnelProtocolKind, TunnelTarget};
use crate::{RouteId, TunnelError};

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing; the configuration still behaves as written.
    Info,
    /// The configuration is accepted, but something will not work as it reads.
    Warning,
    /// The configuration is rejected: starting would fail or produce a route
    /// nobody can reach.
    Error,
}

impl Severity {
    /// Short label for CLI output.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One preflight result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// How serious it is.
    pub severity: Severity,
    /// Which part of the configuration it concerns (`routes[web]`, `agent`, …).
    pub scope: String,
    /// What was found, in operator terms.
    pub message: String,
}

impl Finding {
    fn new(severity: Severity, scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity,
            scope: scope.into(),
            message: message.into(),
        }
    }

    /// An error finding.
    pub fn error(scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, scope, message)
    }

    /// A warning finding.
    pub fn warning(scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, scope, message)
    }

    /// An informational finding.
    pub fn info(scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, scope, message)
    }
}

/// The result of a preflight run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerificationReport {
    /// Findings, most serious first (stable within a severity).
    pub findings: Vec<Finding>,
}

impl VerificationReport {
    /// True when nothing reached [`Severity::Error`].
    pub fn is_ok(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    /// True when nothing reached [`Severity::Warning`] either.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Count of findings at one severity.
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }

    fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Sorts findings so an operator reads the blocking ones first.
    fn sort(&mut self) {
        self.findings
            .sort_by(|left, right| right.severity.cmp(&left.severity));
    }
}

/// Verifies a whole tunnel configuration offline.
///
/// `environment` is consulted only for the *presence* of credential variables
/// when `check_environment` is true; a verification run in CI may pass `false`
/// to check shape without demanding the deployment's secrets.
pub fn verify_config(config: &TunnelConfig, check_environment: bool) -> VerificationReport {
    let mut report = VerificationReport::default();

    if !config.enabled {
        report.push(Finding::info(
            "tunnel",
            "tunnel capability is disabled; routes would not start",
        ));
    }

    let routes = config.routes_or_empty();
    if routes.is_empty() {
        report.push(Finding::warning(
            "tunnel",
            "no routes are declared; a connector with no routes registers nothing",
        ));
    }

    for (index, template) in routes.iter().enumerate() {
        verify_template(template, index, check_environment, &mut report);
    }
    verify_route_interactions(&routes, &mut report);

    if let Some(gateway) = config.gateway.as_ref() {
        if gateway.route_port_min > gateway.route_port_max {
            report.push(Finding::error(
                "gateway",
                format!(
                    "routePortMin {} is above routePortMax {}; no port could ever be registered",
                    gateway.route_port_min, gateway.route_port_max
                ),
            ));
        }
        if let Some(port) = gateway.tcpmux_port {
            if (gateway.route_port_min..=gateway.route_port_max).contains(&port) {
                report.push(Finding::warning(
                    "gateway",
                    format!(
                        "tcpmuxPort {port} is inside the registrable route range {}–{}: an agent could ask for the multiplexer's own port",
                        gateway.route_port_min, gateway.route_port_max
                    ),
                ));
            }
        }
        if gateway
            .agent_token_env
            .iter()
            .all(|name| name.trim().is_empty())
        {
            report.push(Finding::error(
                "gateway",
                "no agentTokenEnv is configured; every agent would be refused",
            ));
        } else if check_environment
            && gateway.agent_token_env.iter().all(|name| {
                std::env::var(name)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            })
        {
            report.push(Finding::warning(
                "gateway",
                format!(
                    "none of {:?} is set; the gateway would start without any accepted agent token",
                    gateway.agent_token_env
                ),
            ));
        }
    }

    report.sort();
    report
}

/// Verifies one route template.
fn verify_template(
    template: &TunnelRouteTemplate,
    index: usize,
    check_environment: bool,
    report: &mut VerificationReport,
) {
    let scope = format!("routes[{}:{}]", index, template.name);

    // Structural validation first: everything below assumes a well-formed
    // route, and a second error about a target that never parsed is noise.
    if let Err(error) = template.validate() {
        report.push(Finding::error(scope, error.to_string()));
        return;
    }

    match template.target() {
        Ok(TunnelTarget::Tcp(address)) => {
            report.push(Finding::info(
                scope.clone(),
                format!("relays to the local stream socket {address}"),
            ));
        }
        Ok(TunnelTarget::Udp(address)) => {
            report.push(Finding::info(
                scope.clone(),
                format!("relays to the local datagram socket {address}"),
            ));
        }
        Ok(TunnelTarget::UnixSocket(path)) => {
            report.push(Finding::info(
                scope.clone(),
                format!("relays to the local unix socket {}", path.display()),
            ));
            if template.protocol.is_datagram() {
                // Unreachable through `validate`, but a preflight must not
                // depend on another check having run first.
                report.push(Finding::error(
                    scope.clone(),
                    "a datagram route cannot target a unix socket",
                ));
            }
        }
        Err(error) => report.push(Finding::error(scope.clone(), error.to_string())),
    }

    // The public surface an operator expects, spelled out: this is the line a
    // reader compares against the documentation.
    match (
        &template.protocol,
        template.domain.as_deref(),
        template.port,
    ) {
        (TunnelProtocolKind::Http, Some(domain), _) => report.push(Finding::info(
            scope.clone(),
            format!("public surface: https://{domain} (host-routed)"),
        )),
        (kind, _, Some(port)) if !kind.is_secret() => report.push(Finding::info(
            scope.clone(),
            format!("public surface: gateway port {port}"),
        )),
        (kind, _, _) if kind.is_secret() => report.push(Finding::info(
            scope.clone(),
            format!(
                "public surface: none — visitors ask for `{}` with the shared key",
                template.name
            ),
        )),
        _ => {}
    }

    // Credentials are resolved from the environment, so a preflight that does
    // not check them is only half a preflight.
    if template.protocol.is_secret() {
        match template.secret_key_env.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => {
                if check_environment {
                    match std::env::var(name) {
                        Ok(value) => {
                            let key = value.trim();
                            if key.len() < 8 {
                                report.push(Finding::error(
                                    scope.clone(),
                                    format!(
                                        "`{name}` holds a {}-character key; at least 8 are required",
                                        key.len()
                                    ),
                                ));
                            } else {
                                report.push(Finding::info(
                                    scope.clone(),
                                    format!(
                                        "shared key resolved from `{name}` ({}-character key)",
                                        key.len()
                                    ),
                                ));
                            }
                        }
                        Err(_) => report.push(Finding::error(
                            scope.clone(),
                            format!(
                                "`{name}` is not set; the route cannot authenticate any visitor"
                            ),
                        )),
                    }
                } else {
                    report.push(Finding::info(
                        scope.clone(),
                        format!("shared key comes from `{name}` (environment not checked)"),
                    ));
                }
            }
            _ => report.push(Finding::error(
                scope.clone(),
                "a secret route needs secretKeyEnv naming the variable that holds its key",
            )),
        }
    }

    // Header rewriting is reported because it changes what the service behind
    // the tunnel observes — the operator should see the policy in the same
    // preflight that lists the public surface (FRP `hostHeaderRewrite` /
    // `requestHeaders.set` / `responseHeaders.set`).
    match template.header_rewrite() {
        Ok(rewrite) if !rewrite.is_empty() => {
            if let Some(host) = rewrite.host() {
                report.push(Finding::info(
                    scope.clone(),
                    format!("request Host rewritten to `{host}`"),
                ));
            }
            for (direction, pairs) in [
                ("request", rewrite.request_set()),
                ("response", rewrite.response_set()),
            ] {
                if pairs.is_empty() {
                    continue;
                }
                let names = pairs
                    .iter()
                    .map(|pair| pair.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                report.push(Finding::info(
                    scope.clone(),
                    format!("{direction} headers set at the gateway: {names}"),
                ));
            }
        }
        Ok(_) => {}
        Err(error) => report.push(Finding::error(scope.clone(), error.to_string())),
    }

    if let Some(health) = template.health_check.as_ref() {
        if let Err(error) = health.validate() {
            report.push(Finding::error(scope.clone(), error.to_string()));
        } else {
            report.push(Finding::info(
                scope.clone(),
                format!(
                    "{} probe every {}s (timeout {}s, {} failure(s) mark it down){}",
                    health.kind.label(),
                    health.interval_seconds,
                    health.timeout_seconds,
                    health.max_failed,
                    match health.kind {
                        crate::health::HealthCheckKind::Http =>
                            format!(" at {}", health.request_path()),
                        crate::health::HealthCheckKind::Tcp => String::new(),
                    }
                ),
            ));
        }
    } else if template.load_balancer_group.is_some() {
        // A group member without a probe never leaves the rotation: the
        // gateway would keep sending it traffic after its service died.
        report.push(Finding::warning(
            scope.clone(),
            "a load-balancer group member without a healthCheck keeps receiving traffic after its service stops",
        ));
    }

    if template.bandwidth_limit_bytes_per_second == Some(0) {
        report.push(Finding::error(
            scope,
            "a bandwidth limit of 0 would stall the relay completely",
        ));
    }
}

/// Verifies how the routes in one configuration interact.
///
/// These are the failures that only appear once a gateway is involved — two
/// routes claiming one surface, or a group whose members do not share one — and
/// they are exactly what an offline preflight is for.
fn verify_route_interactions(routes: &[TunnelRouteTemplate], report: &mut VerificationReport) {
    // Duplicate names would collide on the derived route id at registration.
    let mut seen_names: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for template in routes {
        *seen_names.entry(template.name.as_str()).or_insert(0) += 1;
    }
    for (name, count) in seen_names.iter().filter(|(_, count)| **count > 1) {
        report.push(Finding::error(
            "routes",
            format!("route name `{name}` is declared {count} times; route ids must be unique"),
        ));
    }

    // Two routes on one surface with no group: the second registration is
    // refused by the gateway, which is late and confusing.
    for (index, template) in routes.iter().enumerate() {
        for other in routes.iter().skip(index + 1) {
            if template.name == other.name {
                continue;
            }
            let surface = match (&template.protocol, &other.protocol) {
                (TunnelProtocolKind::Http, TunnelProtocolKind::Http) => template
                    .domain
                    .as_deref()
                    .zip(other.domain.as_deref())
                    .filter(|(left, right)| left.eq_ignore_ascii_case(right))
                    .map(|(domain, _)| format!("domain {domain}")),
                (left, right)
                    if !left.is_secret()
                        && !right.is_secret()
                        && left.is_datagram() == right.is_datagram() =>
                {
                    template
                        .port
                        .zip(other.port)
                        .filter(|(left, right)| left == right)
                        .map(|(port, _)| format!("gateway port {port}"))
                }
                _ => None,
            };
            let Some(surface) = surface else {
                continue;
            };
            let share_a_group = template.load_balancer_group.is_some()
                && template.load_balancer_group == other.load_balancer_group;
            if share_a_group {
                continue;
            }
            // Disjoint location prefixes are how two routes legitimately share
            // one hostname.
            let disjoint_locations = !template.locations.is_empty()
                && !other.locations.is_empty()
                && template.locations.iter().all(|left| {
                    other
                        .locations
                        .iter()
                        .all(|right| !locations_overlap(left, right))
                });
            if disjoint_locations {
                continue;
            }
            report.push(Finding::error(
                "routes",
                format!(
                    "`{}` and `{}` both claim {surface} without a shared loadBalancerGroup; the second registration would be refused",
                    template.name, other.name
                ),
            ));
        }
    }

    // Members of one group must agree on the surface, or they are replicas of
    // nothing: the gateway can only select between routes that match the same
    // request.
    let mut groups: std::collections::HashMap<&str, Vec<&TunnelRouteTemplate>> =
        std::collections::HashMap::new();
    for template in routes {
        if let Some(group) = template.load_balancer_group.as_deref() {
            groups.entry(group).or_default().push(template);
        }
    }
    for (group, members) in groups {
        let surfaces: std::collections::HashSet<String> = members
            .iter()
            .map(|template| {
                match (
                    &template.protocol,
                    template.domain.as_deref(),
                    template.port,
                ) {
                    (_, Some(domain), _) => format!("domain:{}", domain.to_ascii_lowercase()),
                    (_, _, Some(port)) => format!("port:{port}"),
                    _ => format!("secret:{}", template.name),
                }
            })
            .collect();
        if surfaces.len() > 1 {
            report.push(Finding::error(
                "routes",
                format!(
                    "loadBalancerGroup `{group}` members do not share one surface ({:?}); the gateway can never select between them",
                    surfaces
                ),
            ));
        }
    }

    // Route ids are derived from names; a name that cannot form one is caught
    // per-route, but the derivation itself is worth confirming here.
    for template in routes {
        if RouteId::parse(format!("route_{}", template.name)).is_err() {
            report.push(Finding::error(
                "routes",
                format!("route name `{}` cannot form a route id", template.name),
            ));
        }
    }
}

/// Whether two location prefixes can both serve one request.
fn locations_overlap(left: &str, right: &str) -> bool {
    let normalize = |value: &str| {
        let trimmed = value.trim();
        let trimmed = trimmed.strip_suffix('/').unwrap_or(trimmed);
        if trimmed.is_empty() {
            "/".to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    let left = normalize(left);
    let right = normalize(right);
    left == right
        || left == "/"
        || right == "/"
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

/// Verifies one route template in isolation (used by `--route` style CLIs).
pub fn verify_template_standalone(
    template: &TunnelRouteTemplate,
    check_environment: bool,
) -> VerificationReport {
    let mut report = VerificationReport::default();
    verify_template(template, 0, check_environment, &mut report);
    report.sort();
    report
}

/// Converts a report into the domain error type when it contains errors.
pub fn report_to_result(report: &VerificationReport) -> crate::Result<()> {
    if report.is_ok() {
        return Ok(());
    }
    let errors: Vec<String> = report
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .map(|finding| format!("{}: {}", finding.scope, finding.message))
        .collect();
    Err(TunnelError::Validation {
        field: crate::ValidationField::Config,
        reason: errors.join("; "),
    })
}

/// The matcher a verified template would register (used by reports and tests).
pub fn registered_matcher(template: &TunnelRouteTemplate) -> Option<RouteMatcher> {
    template.matcher().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TunnelRouteTemplate;

    fn http_template(name: &str, domain: &str, target: &str) -> TunnelRouteTemplate {
        TunnelRouteTemplate {
            name: name.to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some(domain.to_owned()),
            target: target.to_owned(),
            ..TunnelRouteTemplate::default()
        }
    }

    fn report_for(routes: Vec<TunnelRouteTemplate>) -> VerificationReport {
        let mut report = VerificationReport::default();
        for (index, template) in routes.iter().enumerate() {
            verify_template(template, index, false, &mut report);
        }
        verify_route_interactions(&routes, &mut report);
        report.sort();
        report
    }

    #[test]
    fn a_healthy_configuration_reports_its_public_surface() {
        let report = report_for(vec![http_template(
            "web",
            "demo.sdkwork.link",
            "127.0.0.1:3000",
        )]);
        assert!(report.is_ok(), "{:?}", report.findings);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("https://demo.sdkwork.link")),
            "the report must state the surface the operator is publishing: {:?}",
            report.findings
        );
    }

    #[test]
    fn two_routes_on_one_surface_without_a_group_are_an_error() {
        let report = report_for(vec![
            http_template("web", "demo.sdkwork.link", "127.0.0.1:3000"),
            http_template("api", "DEMO.sdkwork.link", "127.0.0.1:3001"),
        ]);
        assert!(!report.is_ok());
        assert!(
            report.findings.iter().any(|finding| finding
                .message
                .contains("both claim domain demo.sdkwork.link")),
            "{:?}",
            report.findings
        );

        // Disjoint paths make the same host legitimate, and a shared group
        // makes it a replica set.
        let mut left = http_template("web", "demo.sdkwork.link", "127.0.0.1:3000");
        left.locations = vec!["/api".to_owned()];
        let mut right = http_template("static", "demo.sdkwork.link", "127.0.0.1:3001");
        right.locations = vec!["/static".to_owned()];
        let split = report_for(vec![left, right]);
        assert!(split.is_ok(), "{:?}", split.findings);

        let mut first = http_template("web-a", "demo.sdkwork.link", "127.0.0.1:3000");
        first.load_balancer_group = Some("web".to_owned());
        let mut second = http_template("web-b", "demo.sdkwork.link", "127.0.0.1:3001");
        second.load_balancer_group = Some("web".to_owned());
        let grouped = report_for(vec![first, second]);
        assert!(grouped.is_ok(), "{:?}", grouped.findings);

        // A group whose members do not share a surface is a group in name only.
        let mut third = http_template("web-c", "other.sdkwork.link", "127.0.0.1:3002");
        third.load_balancer_group = Some("web".to_owned());
        let mut fourth = http_template("web-d", "demo.sdkwork.link", "127.0.0.1:3003");
        fourth.load_balancer_group = Some("web".to_owned());
        let mut mixed = VerificationReport::default();
        for (index, template) in [&third, &fourth].iter().enumerate() {
            verify_template(template, index, false, &mut mixed);
        }
        verify_route_interactions(&[third, fourth], &mut mixed);
        mixed.sort();
        assert!(!mixed.is_ok());
        assert!(
            mixed
                .findings
                .iter()
                .any(|finding| finding.message.contains("do not share one surface")),
            "{:?}",
            mixed.findings
        );
    }

    #[test]
    fn a_group_member_without_a_probe_is_a_warning() {
        let mut template = http_template("web-a", "demo.sdkwork.link", "127.0.0.1:3000");
        template.load_balancer_group = Some("web".to_owned());
        let report = report_for(vec![template]);
        assert!(report.is_ok(), "a warning must not fail the run");
        assert_eq!(report.count(Severity::Warning), 1);
        assert!(
            report.findings[0].message.contains("without a healthCheck"),
            "{:?}",
            report.findings
        );
    }

    #[test]
    fn duplicate_names_and_unusable_targets_are_errors() {
        let report = report_for(vec![
            http_template("web", "a.sdkwork.link", "127.0.0.1:3000"),
            http_template("web", "b.sdkwork.link", "127.0.0.1:3001"),
        ]);
        assert!(!report.is_ok());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("declared 2 times")),
            "{:?}",
            report.findings
        );

        let mut bad = http_template("broken", "c.sdkwork.link", "not-an-address");
        bad.port = None;
        let report = report_for(vec![bad]);
        assert!(!report.is_ok());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.message.contains("IP:port")));
    }

    #[test]
    fn report_to_result_surfaces_every_error_at_once() {
        let report = report_for(vec![
            http_template("web", "a.sdkwork.link", "127.0.0.1:3000"),
            http_template("api", "a.sdkwork.link", "127.0.0.1:3001"),
        ]);
        let error = report_to_result(&report).expect_err("errors fail the run");
        let text = error.to_string();
        assert!(text.contains("both claim"), "{text}");

        let clean = report_for(vec![http_template(
            "web",
            "a.sdkwork.link",
            "127.0.0.1:3000",
        )]);
        assert!(report_to_result(&clean).is_ok());
    }

    #[test]
    fn gateway_limits_are_checked_for_self_consistency() {
        let mut config = TunnelConfig::disabled();
        config.enabled = true;
        config.routes = Some(vec![http_template(
            "web",
            "a.sdkwork.link",
            "127.0.0.1:3000",
        )]);

        // An inverted port range would refuse every port an agent asks for.
        config.gateway = Some(crate::config::TunnelGatewayConfig {
            route_port_min: 40_000,
            route_port_max: 30_000,
            ..crate::config::TunnelGatewayConfig::default()
        });
        let report = verify_config(&config, false);
        assert!(!report.is_ok());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.message.contains("above routePortMax")));

        // A multiplexer port inside the registrable range is a warning, not an
        // error: it is a policy choice, not a broken configuration.
        config.gateway = Some(crate::config::TunnelGatewayConfig {
            route_port_min: 1024,
            route_port_max: 65_535,
            tcpmux_port: Some(8444),
            ..crate::config::TunnelGatewayConfig::default()
        });
        let report = verify_config(&config, false);
        assert!(report.is_ok(), "{:?}", report.findings);
        assert!(report.findings.iter().any(|finding| finding
            .message
            .contains("inside the registrable route range")));

        // The default range keeps an operator's multiplexer port out of reach,
        // and the report says nothing about it: the default range starts at
        // 1024, so a multiplexer on a low port is out of agent reach.
        config.gateway = Some(crate::config::TunnelGatewayConfig {
            route_port_min: 9000,
            route_port_max: 65_535,
            tcpmux_port: Some(8444),
            ..crate::config::TunnelGatewayConfig::default()
        });
        let report = verify_config(&config, false);
        assert!(report.is_ok(), "{:?}", report.findings);
        assert!(
            !report.findings.iter().any(|finding| finding
                .message
                .contains("inside the registrable route range")),
            "an unreachable multiplexer port is not a finding: {:?}",
            report.findings
        );
    }

    #[test]
    fn locations_overlap_knows_prefixes() {
        assert!(locations_overlap("/api", "/api"));
        assert!(locations_overlap("/api", "/api/v2"));
        assert!(locations_overlap("/", "/anything"));
        assert!(!locations_overlap("/api", "/static"));
        assert!(locations_overlap("/api/", "/api"));
    }
}
