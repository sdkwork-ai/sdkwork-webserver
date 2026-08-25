//! Merge independently materialized nginx site files into one runtime app.
//!
//! Sites share listeners by bind+port. TLS certificates from later sites are
//! folded into the first listener's policy so SNI can select the right
//! certificate (nginx multi-`server` on 443). Virtual hosts stay attached.

use std::collections::HashMap;

use crate::config::{
    ListenerConfig, ResourceConfig, TlsPolicyConfig, WebServerAppConfig, WebServerConfigError,
};

/// Merge `right` into `left`. Resource/route ids from `right` are rewritten
/// so they cannot collide with `left`. Same-socket TLS listeners combine
/// certificate refs instead of dropping the later virtual hosts.
pub fn merge_nginx_apps(
    mut left: WebServerAppConfig,
    right: WebServerAppConfig,
) -> Result<WebServerAppConfig, WebServerConfigError> {
    let id_suffix = left.resources.len();
    let mut right = right;
    for resource in &mut right.resources {
        resource.set_id(format!("{}-{id_suffix}", resource.id()));
    }
    for host in &mut right.virtual_hosts {
        for route in &mut host.routes {
            route.id = format!("{}-{id_suffix}", route.id);
            route.resource_ref = format!("{}-{id_suffix}", route.resource_ref);
        }
    }

    let mut renamed_upstreams = HashMap::new();
    for upstream in &right.upstreams {
        if left
            .upstreams
            .iter()
            .any(|existing| existing.id == upstream.id)
        {
            let unique = format!("{}-{}", upstream.id, left.upstreams.len());
            renamed_upstreams.insert(upstream.id.clone(), unique.clone());
            let mut copy = upstream.clone();
            copy.id = unique;
            left.upstreams.push(copy);
        } else {
            left.upstreams.push(upstream.clone());
        }
    }
    // Merge shared listeners before consuming `right` fields that move,
    // so certificate refs from the incoming site can still be borrowed.
    let mut listener_id_map = HashMap::new();
    for right_listener in &right.listeners {
        if let Some(index) = left.listeners.iter().position(|existing| {
            existing.bind.eq_ignore_ascii_case(&right_listener.bind)
                && existing.port == right_listener.port
        }) {
            listener_id_map.insert(right_listener.id.clone(), left.listeners[index].id.clone());
            merge_shared_listener(&mut left, index, right_listener, &right);
        } else {
            left.listeners.push(right_listener.clone());
        }
    }

    for mut resource in right.resources {
        if let Some(unique) = resource_upstream_ref(&resource)
            .and_then(|current| renamed_upstreams.get(current))
        {
            resource.set_proxy_upstream_ref(unique.clone());
        }
        left.resources.push(resource);
    }

    for mut host in right.virtual_hosts {
        for listener_ref in &mut host.listener_refs {
            if let Some(mapped) = listener_id_map.get(listener_ref) {
                *listener_ref = mapped.clone();
            }
        }
        host.listener_refs.sort();
        host.listener_refs.dedup();
        if !host.listener_refs.is_empty() {
            left.virtual_hosts.push(host);
        }
    }

    for certificate in right.certificates {
        if !left
            .certificates
            .iter()
            .any(|existing| existing.id == certificate.id)
        {
            left.certificates.push(certificate);
        }
    }
    for policy in right.tls_policies {
        if !left
            .tls_policies
            .iter()
            .any(|existing| existing.id == policy.id)
        {
            left.tls_policies.push(policy);
        }
    }
    for zone in right.limit_req_zones {
        if !left
            .limit_req_zones
            .iter()
            .any(|existing| existing.name == zone.name)
        {
            left.limit_req_zones.push(zone);
        }
    }
    for stream in right.streams {
        if left.streams.iter().any(|existing| existing.id == stream.id) {
            let mut copy = stream;
            copy.id = format!("{}-{}", copy.id, left.streams.len());
            left.streams.push(copy);
        } else {
            left.streams.push(stream);
        }
    }

    if right.gzip.enabled {
        left.gzip.enabled = true;
    }
    for mime in right.gzip.types {
        if !left.gzip.types.contains(&mime) {
            left.gzip.types.push(mime);
        }
    }
    if right.gzip.min_length > 0 {
        if left.gzip.min_length == 0 {
            left.gzip.min_length = right.gzip.min_length;
        } else {
            left.gzip.min_length = left.gzip.min_length.min(right.gzip.min_length);
        }
    }
    if right.proxy_cache.enabled {
        left.proxy_cache.enabled = true;
    }
    left.proxy_cache.max_entries = left
        .proxy_cache
        .max_entries
        .max(right.proxy_cache.max_entries);
    left.proxy_cache.max_object_bytes = left
        .proxy_cache
        .max_object_bytes
        .max(right.proxy_cache.max_object_bytes);
    left.proxy_cache.default_ttl_seconds = left
        .proxy_cache
        .default_ttl_seconds
        .max(right.proxy_cache.default_ttl_seconds);
    if left.proxy_cache.disk_path.is_none() {
        left.proxy_cache.disk_path = right.proxy_cache.disk_path;
    }

    crate::config::validate_webserver_config(&left)?;
    Ok(left)
}

fn resource_upstream_ref(resource: &ResourceConfig) -> Option<&str> {
    match resource {
        ResourceConfig::Proxy { upstream_ref, .. } => Some(upstream_ref.as_str()),
        _ => None,
    }
}

fn merge_shared_listener(
    left: &mut WebServerAppConfig,
    left_index: usize,
    right_listener: &ListenerConfig,
    right: &WebServerAppConfig,
) {
    for protocol in &right_listener.protocols {
        if !left.listeners[left_index].protocols.contains(protocol) {
            left.listeners[left_index].protocols.push(*protocol);
        }
    }
    // nginx default server: the first file loaded owns the listener default;
    // a later file's default only applies when the first declared none.
    if left.listeners[left_index].default_virtual_host_ref.is_none() {
        left.listeners[left_index].default_virtual_host_ref =
            right_listener.default_virtual_host_ref.clone();
    }
    let left_policy_id = left.listeners[left_index].tls_policy_ref.clone();
    let right_policy_id = right_listener.tls_policy_ref.clone();
    match (left_policy_id, right_policy_id) {
        (Some(left_id), Some(right_id)) if left_id != right_id => {
            let right_refs = certificate_refs_of(right, &right_id);
            if let Some(policy) = left
                .tls_policies
                .iter_mut()
                .find(|policy| policy.id == left_id)
            {
                extend_certificate_refs(policy, right_refs);
            }
        }
        (None, Some(right_id)) => {
            left.listeners[left_index].tls_policy_ref = Some(right_id);
            left.listeners[left_index].allow_plaintext_http = false;
        }
        _ => {}
    }
}

fn certificate_refs_of(app: &WebServerAppConfig, policy_id: &str) -> Vec<String> {
    app.tls_policies
        .iter()
        .find(|policy| policy.id == policy_id)
        .map(|policy| {
            policy
                .certificate_refs()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extend_certificate_refs(policy: &mut TlsPolicyConfig, refs: Vec<String>) {
    for cert in refs {
        let already = policy
            .certificate_ref
            .as_deref()
            .is_some_and(|existing| existing == cert)
            || policy.certificate_refs.iter().any(|existing| existing == &cert);
        if !already {
            policy.certificate_refs.push(cert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nginx::{materialize_nginx_app, parse_nginx_config};

    fn app(text: &str) -> WebServerAppConfig {
        let parsed = parse_nginx_config(text, std::path::Path::new("site.conf")).expect("parse");
        materialize_nginx_app(
            &parsed,
            std::path::Path::new("/etc/nginx/sites-enabled"),
            "test",
        )
        .expect("materialize")
    }

    #[test]
    fn merge_keeps_both_tls_virtual_hosts_on_shared_443() {
        let left = app(
            r#"
server {
    listen 443 ssl http2;
    server_name api.example.com;
    ssl_certificate /etc/ssl/api.pem;
    ssl_certificate_key /etc/ssl/api.key;
    location / { proxy_pass http://127.0.0.1:3913; }
}
"#,
        );
        let right = app(
            r#"
server {
    listen 443 ssl http2;
    server_name web.example.com;
    ssl_certificate /etc/ssl/web.pem;
    ssl_certificate_key /etc/ssl/web.key;
    location / { proxy_pass http://127.0.0.1:18080; }
}
"#,
        );
        let merged = merge_nginx_apps(left, right).expect("merge");
        assert_eq!(merged.virtual_hosts.len(), 2, "both TLS vhosts must remain");
        assert_eq!(merged.listeners.iter().filter(|l| l.port == 443).count(), 1);
        let policy_id = merged
            .listeners
            .iter()
            .find(|listener| listener.port == 443)
            .and_then(|listener| listener.tls_policy_ref.as_deref())
            .expect("tls policy");
        let policy = merged
            .tls_policies
            .iter()
            .find(|policy| policy.id == policy_id)
            .expect("policy");
        let certs = policy.certificate_refs().collect::<Vec<_>>();
        assert!(certs.len() >= 2, "SNI policy must reference both certificates: {certs:?}");
    }

    #[test]
    fn merge_concatenates_stream_servers() {
        let left = app(
            r#"
stream {
    server { listen 5100; proxy_pass 127.0.0.1:15100; }
}
"#,
        );
        let right = app(
            r#"
stream {
    server { listen 5101; proxy_pass 127.0.0.1:15101; }
}
"#,
        );
        let merged = merge_nginx_apps(left, right).expect("merge");
        assert_eq!(merged.streams.len(), 2);
    }
}
