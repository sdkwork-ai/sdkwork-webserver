//! Nginx deploy, validate, and reload orchestration through the edge runtime.

use std::net::IpAddr;

use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};
use sdkwork_webserver_core::upstream_ip_is_allowed;

use crate::WebService;

/// Proxy-family directives whose targets are scanned for private, loopback,
/// metadata, and unix-socket destinations. Variable-based targets are never
/// approved because they cannot be verified statically.
const PROXY_DIRECTIVES: &[&str] = &[
    "proxy_pass",
    "grpc_pass",
    "fastcgi_pass",
    "uwsgi_pass",
    "scgi_pass",
    "memcached_pass",
];

impl WebService {
    pub async fn validate_nginx_content(&self, content: &str) -> WebServiceResult<()> {
        let risks = scan_nginx_directive_risks(content);
        if let Some(risk) = risks.first() {
            return Err(WebServiceError::validation(format!(
                "nginx configuration is not approved for activation: {risk}"
            )));
        }
        let runtime = self.edge_runtime.clone();
        let content = content.to_owned();
        tokio::task::spawn_blocking(move || runtime.validate_config_content(&content))
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx validation: {error}")))?
            .map_err(|error| WebServiceError::validation(error.to_string()))
    }

    pub async fn deploy_nginx_site(&self, domain: &str, content: &str) -> WebServiceResult<()> {
        let runtime = self.edge_runtime.clone();
        let domain = domain.to_owned();
        let content = content.to_owned();
        tokio::task::spawn_blocking(move || runtime.deploy_app_config(&domain, &content))
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx deployment: {error}")))?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    pub async fn reload_nginx_runtime(&self) -> WebServiceResult<()> {
        let runtime = self.edge_runtime.clone();
        tokio::task::spawn_blocking(move || runtime.reload())
            .await
            .map_err(|error| WebServiceError::Internal(format!("join nginx reload: {error}")))?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    /// Proves the running Nginx master actually serves the deployed revision
    /// (PRD-FR-020): `nginx -T` must contain the site's server-name fragment.
    /// A reload that failed validation keeps the previous revision serving,
    /// so this check fails instead of reporting a false success.
    pub async fn verify_nginx_served(&self, expected_fragment: &str) -> WebServiceResult<()> {
        let runtime = self.edge_runtime.clone();
        let fragment = expected_fragment.to_owned();
        tokio::task::spawn_blocking(move || runtime.verify_served_config(&fragment))
            .await
            .map_err(|error| {
                WebServiceError::Internal(format!("join nginx served verification: {error}"))
            })?
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }
}

/// Scans operator-managed Nginx site content for directives that are never
/// approved for activation. This is a fail-closed static gate in front of
/// `nginx -t`; it blocks arbitrary file inclusion, path-escape aliases,
/// proxies to loopback/private/metadata literal addresses, variable-based
/// proxy targets, and unix-socket upstreams. Hostname targets are resolved by
/// the Nginx worker at runtime, so the deployment environment must
/// additionally confine Nginx outbound traffic to approved networks.
fn scan_nginx_directive_risks(content: &str) -> Vec<String> {
    let mut risks = Vec::new();
    // Braces delimit blocks, not statements; normalize them into statement
    // terminators so directives inside blocks are scanned independently.
    let normalized = content.replace(['{', '}'], ";\n");
    for statement in normalized.split(';') {
        let mut tokens = statement.split_whitespace();
        let Some(directive) = tokens.next() else {
            continue;
        };
        match directive {
            "include" => risks.push(
                "the include directive is forbidden; configuration must be self-contained"
                    .to_string(),
            ),
            "alias" => risks.push(
                "the alias directive is forbidden; static roots are confined to managed locations"
                    .to_string(),
            ),
            // Variable assignments can feed proxied destinations at runtime
            // (for example `set $target http://169.254.169.254; proxy_pass
            // $target;`), so URL literals assigned to variables are scanned
            // for forbidden targets just like direct proxy arguments.
            "set" | "map" => {
                let value = tokens.collect::<Vec<_>>().join(" ");
                if let Some(risk) = variable_assignment_risk(&value) {
                    risks.push(risk);
                }
            }
            // Upstream pool members are declared with `server <address>;`
            // inside `upstream {}` blocks; scan their literal addresses.
            "server" => {
                let value = tokens.collect::<Vec<_>>().join(" ");
                if let Some(risk) = upstream_server_risk(&value) {
                    risks.push(risk);
                }
            }
            directive if PROXY_DIRECTIVES.contains(&directive) => {
                if let Some(argument) = tokens.next() {
                    if let Some(risk) = proxy_pass_risk(argument) {
                        risks.push(risk);
                    }
                }
            }
            _ => {}
        }
    }
    risks
}

/// Scans the value assigned by `set`/`map` for http(s) URLs whose host is a
/// forbidden address. Variable proxies are rejected separately, but a literal
/// forbidden URL assigned to a variable would otherwise survive until it is
/// interpolated at runtime.
fn variable_assignment_risk(value: &str) -> Option<String> {
    let mut risk = None;
    for candidate in split_url_literals(value) {
        if let Some(found) = proxy_pass_risk(candidate) {
            risk = Some(format!(
                "variable assignment embeds a forbidden URL target: {found}"
            ));
            break;
        }
    }
    risk
}

/// Scans an `upstream {}` member declaration (`server <address>;`) for
/// loopback/private/metadata literal addresses and unix sockets.
fn upstream_server_risk(arguments: &str) -> Option<String> {
    let address = arguments.split_whitespace().next()?;
    if address.starts_with("unix:") {
        return Some(
            "upstream unix sockets are forbidden; upstreams must not reach local services"
                .to_string(),
        );
    }
    let host = address
        .split(['/', ':', '?'])
        .next()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    if host.is_empty() {
        return None;
    }
    if host.eq_ignore_ascii_case("localhost") {
        return Some(
            "upstream localhost is forbidden; management surfaces must not be reachable"
                .to_string(),
        );
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !upstream_ip_is_allowed(ip, &[]) {
            return Some(format!("upstream target {host} is not an allowed address"));
        }
    }
    None
}

/// Splits a directive value into `scheme://host...` URL literal candidates so
/// embedded URLs (such as in `set` assignments) are scanned individually.
fn split_url_literals(value: &str) -> Vec<&str> {
    let mut candidates = Vec::new();
    let mut remaining = value;
    while let Some(start) = remaining.find("://") {
        // Back up to the scheme start (http, https, grpc).
        let scheme_start = remaining[..start]
            .rfind(|ch: char| !ch.is_ascii_alphanumeric())
            .map(|index| index + 1)
            .unwrap_or(0);
        let literal = &remaining[scheme_start..];
        let end = literal
            .find(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '"' | '\''))
            .unwrap_or(literal.len());
        let candidate = &literal[..end];
        if candidate.starts_with("http://")
            || candidate.starts_with("https://")
            || candidate.starts_with("grpc://")
        {
            candidates.push(candidate);
        }
        remaining = &literal[end..];
    }
    candidates
}

/// Classifies a proxy-family argument. Literal http/https/grpc URLs are
/// checked for forbidden hosts; scheme-less `host:port` forms (used by
/// fastcgi_pass/uwsgi_pass/...) are checked when the host is an IP literal;
/// variable targets are always rejected because they cannot be statically
/// verified; unix sockets are always rejected.
fn proxy_pass_risk(argument: &str) -> Option<String> {
    if argument.starts_with('$') {
        return Some(
            "proxy targets must be concrete literal URLs; variable targets cannot be statically verified"
                .to_string(),
        );
    }
    if argument.starts_with("unix:") {
        return Some(
            "unix-socket proxy targets are forbidden; management surfaces must not be reachable"
                .to_string(),
        );
    }
    let target = argument
        .strip_prefix("http://")
        .or_else(|| argument.strip_prefix("https://"))
        .or_else(|| argument.strip_prefix("grpc://"))
        // Scheme-less `host:port` form used by fastcgi_pass/uwsgi_pass/...
        .unwrap_or(argument);
    let host = target
        .split(['/', ':', '?'])
        .next()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    if host.is_empty() {
        return Some("proxy target must name a concrete upstream host".to_string());
    }
    if host.eq_ignore_ascii_case("localhost") {
        return Some(
            "proxy to localhost is forbidden; management surfaces must not be reachable"
                .to_string(),
        );
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !upstream_ip_is_allowed(ip, &[]) {
            return Some(format!(
                "proxy target {host} is not an allowed upstream address"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        proxy_pass_risk, scan_nginx_directive_risks, split_url_literals, upstream_server_risk,
        variable_assignment_risk,
    };

    #[test]
    fn risk_scan_rejects_include_alias_and_private_proxies() {
        let safe = "server { listen 80; location / { proxy_pass http://upstream.example.com; } }";
        assert!(scan_nginx_directive_risks(safe).is_empty());

        let risks = scan_nginx_directive_risks("server { include /etc/nginx/secret.conf; }");
        assert!(risks.iter().any(|risk| risk.contains("include")));

        let risks = scan_nginx_directive_risks("server { location /s { alias /etc; } }");
        assert!(risks.iter().any(|risk| risk.contains("alias")));

        let risks = scan_nginx_directive_risks(
            "server { location / { proxy_pass http://169.254.169.254/; } }",
        );
        assert!(risks.iter().any(|risk| risk.contains("169.254.169.254")));

        let risks = scan_nginx_directive_risks(
            "server { location / { proxy_pass http://127.0.0.1:3800; } }",
        );
        assert!(risks.iter().any(|risk| risk.contains("127.0.0.1")));

        let risks =
            scan_nginx_directive_risks("server { location / { proxy_pass http://localhost; } }");
        assert!(risks.iter().any(|risk| risk.contains("localhost")));
    }

    #[test]
    fn proxy_pass_risk_accepts_public_targets_and_rejects_private_literals() {
        assert!(proxy_pass_risk("http://api.example.com/").is_none());
        assert!(proxy_pass_risk("https://example.com:8443/path").is_none());
        assert!(proxy_pass_risk("http://10.0.0.5/").is_some());
        assert!(proxy_pass_risk("http://192.168.1.1:8080/").is_some());
        assert!(proxy_pass_risk("http://[::1]:80/").is_some());
        assert!(proxy_pass_risk("http:///").is_some());
    }

    #[test]
    fn proxy_pass_risk_rejects_variable_targets_and_unix_sockets() {
        // Variable targets cannot be verified statically; `set $u
        // http://169.254.169.254; proxy_pass $u;` must not pass the gate.
        assert!(proxy_pass_risk("$upstream").is_some());
        assert!(proxy_pass_risk("$scheme://$host").is_some());
        assert!(proxy_pass_risk("unix:/var/run/php-fpm.sock").is_some());
    }

    #[test]
    fn proxy_family_directives_are_scanned() {
        for directive in [
            "grpc_pass",
            "fastcgi_pass",
            "uwsgi_pass",
            "scgi_pass",
            "memcached_pass",
        ] {
            let risky = format!("server {{ location / {{ {directive} 127.0.0.1:9000; }} }}");
            let risks = scan_nginx_directive_risks(&risky);
            assert!(
                risks.iter().any(|risk| risk.contains("127.0.0.1")),
                "{directive} must be scanned: {risks:?}"
            );
            let variable = format!("server {{ location / {{ {directive} $backend; }} }}");
            let risks = scan_nginx_directive_risks(&variable);
            assert!(
                risks.iter().any(|risk| risk.contains("variable")),
                "{directive} variable targets must be rejected: {risks:?}"
            );
        }
    }

    #[test]
    fn variable_assignment_urls_are_scanned_for_forbidden_hosts() {
        assert!(variable_assignment_risk("$upstream http://169.254.169.254;").is_some());
        assert!(variable_assignment_risk("$upstream http://localhost;").is_some());
        assert!(variable_assignment_risk("$upstream https://api.example.com;").is_none());
        assert!(variable_assignment_risk("$count 3;").is_none());
    }

    #[test]
    fn scan_rejects_variable_taint_pairs() {
        let risks = scan_nginx_directive_risks(
            "server { set $upstream http://169.254.169.254; location / { proxy_pass $upstream; } }",
        );
        assert!(!risks.is_empty());
        assert!(risks.iter().any(|risk| risk.contains("169.254.169.254")));
    }

    #[test]
    fn upstream_server_members_are_scanned() {
        assert!(upstream_server_risk("127.0.0.1:8080;").is_some());
        assert!(upstream_server_risk("10.0.0.5:80;").is_some());
        assert!(upstream_server_risk("unix:/run/app.sock;").is_some());
        assert!(upstream_server_risk("api.example.com:443;").is_none());
    }

    #[test]
    fn scan_rejects_private_upstream_pool_members() {
        let risks = scan_nginx_directive_risks(
            "upstream backend { server 10.0.0.5:8080; } server { location / { proxy_pass http://backend; } }",
        );
        assert!(risks.iter().any(|risk| risk.contains("10.0.0.5")));
    }

    #[test]
    fn split_url_literals_finds_embedded_urls() {
        assert_eq!(
            split_url_literals("$u http://169.254.169.254 extra https://ok.example.com"),
            vec!["http://169.254.169.254", "https://ok.example.com"]
        );
    }
}
