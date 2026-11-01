//! Per-connection nginx **stream** variable resolution.
//!
//! The bounded engine itself lives in `sdkwork_webserver_core`
//! (`config::stream_variables`); this module supplies the connection facts and
//! folds stream `map` results plus `set` assignments into one value table that
//! the stream handlers (`return`, and future variable consumers) expand.

use std::net::SocketAddr;

use sdkwork_webserver_core::{
    evaluate_split_clients, evaluate_stream_sets, expand_stream_template, resolve_map_value,
    GeoIpDatabases, StreamServerConfig, StreamVariableFacts, WebServerAppConfig,
};

/// Build the per-connection variable table.
///
/// `ssl_preread_server_name` is `None` until the TLS ClientHello has been
/// read; it resolves to the empty string before that, exactly like nginx
/// without `ssl_preread on`.
pub(crate) fn resolve_stream_variables(
    config: &WebServerAppConfig,
    server: &StreamServerConfig,
    peer: SocketAddr,
    local: SocketAddr,
    ssl_preread_server_name: Option<&str>,
    geoip: Option<&GeoIpDatabases>,
) -> Vec<(String, String)> {
    let facts = StreamVariableFacts {
        remote_addr: peer.ip(),
        remote_port: peer.port(),
        server_addr: local.ip(),
        server_port: local.port(),
        ssl_preread_server_name: ssl_preread_server_name.map(str::to_owned),
        geoip: geoip.map_or_else(Default::default, |geoip| geoip.lookup(peer.ip(), &[])),
    };
    let mut map_results = Vec::with_capacity(config.stream_maps.len());
    for map in &config.stream_maps {
        // `evaluate_map` validates its source against the HTTP variable set,
        // which does not include `$ssl_preread_server_name`; the stream engine
        // has already validated the bounded stream sources, so resolve the
        // source value here and use the shared map matcher directly.
        let source = map
            .source
            .strip_prefix('$')
            .and_then(|name| facts.lookup(name))
            .unwrap_or_default();
        let value = resolve_map_value(map, &source);
        if let Some(name) = map.result.strip_prefix('$') {
            map_results.push((name.to_owned(), value.to_owned()));
        }
    }
    // nginx stream `split_clients`: the shared MurmurHash2 bucket engine over
    // the expanded accept/preread source template (the materializer restricted
    // the source to the stream variable vocabulary).
    for split in &config.stream_split_clients {
        let value = evaluate_split_clients(split, &facts);
        if let Some(name) = split.result.strip_prefix('$') {
            map_results.push((name.to_owned(), value));
        }
    }
    evaluate_stream_sets(&server.set_variables, &facts, &map_results)
}

/// Expand one stream template (`set` value or `return` value).
pub(crate) fn expand_stream_value(variables: &[(String, String)], template: &str) -> String {
    expand_stream_template(template, variables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_core::{GeoIpDatabases, StreamServerConfig, WebServerAppConfig};

    fn fixtures() -> (WebServerAppConfig, StreamServerConfig) {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("nginx")
            .join("geoip");
        let config: WebServerAppConfig = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "geoip-stream-test",
            "listeners": [],
            "resources": [],
            "virtualHosts": [],
            "streamGeoip": {
                "country": fixture.join("GeoIP2-Country-Test.mmdb").canonicalize().expect("country").to_string_lossy(),
                "city": fixture.join("GeoIP2-City-Test.mmdb").canonicalize().expect("city").to_string_lossy(),
                "org": fixture.join("GeoIP2-ASN-Test.mmdb").canonicalize().expect("org").to_string_lossy()
            }
        }))
        .expect("config");
        let server: StreamServerConfig = serde_json::from_value(serde_json::json!({
            "id": "stream",
            "bind": "127.0.0.1",
            "port": 19000
        }))
        .expect("stream server");
        (config, server)
    }

    #[test]
    fn resolves_stream_geoip_from_the_accepted_address() {
        let (config, server) = fixtures();
        let databases =
            GeoIpDatabases::load(config.stream_geoip.as_ref().expect("stream geoip"), None)
                .expect("load");
        let variables = resolve_stream_variables(
            &config,
            &server,
            "198.51.100.7:12345".parse().expect("peer"),
            "127.0.0.1:19000".parse().expect("local"),
            None,
            Some(&databases),
        );
        let lookup = |name: &str| {
            variables
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("geoip_country_code"), Some("US"));
        assert_eq!(lookup("geoip_city"), Some("Testville"));
        assert_eq!(lookup("geoip_org"), Some("Example Org"));
        assert_eq!(lookup("geoip_postal_code"), Some("90210"));
    }

    #[test]
    fn stream_split_clients_buckets_the_preread_sni_like_nginx() {
        // The expected assignments for these exact source strings were captured
        // from a real nginx 1.29.6 `split_clients "$src" { 50% "left"; * "right"; }`
        // block (a `set` supplied the source value), so this locks the
        // `$ssl_preread_server_name` wiring against nginx's MurmurHash2
        // thresholds rather than against the runtime's own arithmetic. The
        // accept-time sources are covered by the differential-stream
        // `split_clients` fixture; the pinned oracle lacks ssl_preread.
        let config: WebServerAppConfig = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "kind": "sdkwork.webserver.app",
            "appKey": "stream-split-test",
            "listeners": [],
            "resources": [],
            "virtualHosts": [],
            "streamSplitClients": [{
                "source": "$ssl_preread_server_name",
                "result": "$bucket",
                "parts": [
                    {"percent": 5000, "value": "left"},
                    {"value": "right"}
                ]
            }]
        }))
        .expect("config");
        let server: StreamServerConfig = serde_json::from_value(serde_json::json!({
            "id": "stream",
            "bind": "127.0.0.1",
            "port": 19000
        }))
        .expect("stream server");
        let bucket = |sni: &str| {
            let variables = resolve_stream_variables(
                &config,
                &server,
                "198.51.100.7:12345".parse().expect("peer"),
                "127.0.0.1:19000".parse().expect("local"),
                Some(sni),
                None,
            );
            variables
                .into_iter()
                .find(|(name, _)| name == "bucket")
                .map(|(_, value)| value)
                .expect("split_clients result")
        };
        assert_eq!(bucket("a.example"), "left");
        assert_eq!(bucket("b.example"), "right");
        assert_eq!(bucket("mail.example.com"), "left");
        assert_eq!(bucket("tls-stream.test"), "right");
        assert_eq!(bucket("x"), "left");
        // Without `ssl_preread on` the variable is empty; nginx hashes the
        // empty string (MurmurHash2 of "" is 0) and the first 50% bucket wins
        // (verified against the oracle with `set $src ""`).
        let variables = resolve_stream_variables(
            &config,
            &server,
            "198.51.100.7:12345".parse().expect("peer"),
            "127.0.0.1:19000".parse().expect("local"),
            None,
            None,
        );
        assert!(variables
            .iter()
            .any(|(name, value)| name == "bucket" && value == "left"));
    }

    #[test]
    fn an_address_without_a_record_yields_empty_stream_geoip_variables() {
        let (config, server) = fixtures();
        let databases =
            GeoIpDatabases::load(config.stream_geoip.as_ref().expect("stream geoip"), None)
                .expect("load");
        let variables = resolve_stream_variables(
            &config,
            &server,
            "192.0.2.99:12345".parse().expect("peer"),
            "127.0.0.1:19000".parse().expect("local"),
            None,
            Some(&databases),
        );
        assert!(variables
            .iter()
            .any(|(name, value)| name == "geoip_country_code" && value.is_empty()));
    }
}
