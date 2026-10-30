//! Per-connection nginx **stream** variable resolution.
//!
//! The bounded engine itself lives in `sdkwork_webserver_core`
//! (`config::stream_variables`); this module supplies the connection facts and
//! folds stream `map` results plus `set` assignments into one value table that
//! the stream handlers (`return`, and future variable consumers) expand.

use std::net::SocketAddr;

use sdkwork_webserver_core::{
    evaluate_stream_sets, expand_stream_template, resolve_map_value, StreamServerConfig,
    StreamVariableFacts, WebServerAppConfig,
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
) -> Vec<(String, String)> {
    let facts = StreamVariableFacts {
        remote_addr: peer.ip(),
        remote_port: peer.port(),
        server_addr: local.ip(),
        server_port: local.port(),
        ssl_preread_server_name: ssl_preread_server_name.map(str::to_owned),
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
    evaluate_stream_sets(&server.set_variables, &facts, &map_results)
}

/// Expand one stream template (`set` value or `return` value).
pub(crate) fn expand_stream_value(variables: &[(String, String)], template: &str) -> String {
    expand_stream_template(template, variables)
}
