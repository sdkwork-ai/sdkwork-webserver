//! Bounded nginx **stream** variable engine.
//!
//! The stream plane evaluates variables once per connection from the facts it
//! owns at accept/preread time. This is deliberately smaller than the HTTP map
//! engine: a stream `map` source must be one of the known per-connection values,
//! and map/set/return values are literal templates over those variables plus
//! earlier `map` results and `set` assignments.
//!
//! Known at accept: `$remote_addr`, `$remote_port`, `$server_addr`,
//! `$server_port`, `$binary_remote_addr`. Known after TLS preread:
//! `$ssl_preread_server_name` (empty without `ssl_preread on`).
//!
//! A full stream variable engine (rewrites, `if`, upstream selection by
//! variable, `$proxy_protocol_*` from parsed inbound PROXY headers) is not
//! implemented; those forms fail closed at configuration time.
//!
//! `resolve_stream_pass_target` resolves the nginx stream `pass <address>`
//! content handler: the connection is re-dispatched to the default virtual
//! server of the named listening socket (nginx `ngx_stream_pass_module`).

use std::net::IpAddr;

use super::geoip::GeoIpRequestState;
use super::map_variables::MapVariables;
use super::model::{StreamServerConfig, StreamSetVariableConfig};

/// nginx `NGX_STREAM_PASS_MAX_PASSES`: a pass chain longer than this is a cycle.
pub const STREAM_PASS_MAX_PASSES: u8 = 10;

/// Why a stream `pass <address>` target could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPassError {
    /// The value is not an `IP:port` / `[IPv6]:port` literal.
    InvalidAddress,
    /// No configured stream listener listens on the named address.
    NotConfigured,
    /// The pass chain revisits a server or exceeds nginx's 10-pass bound.
    Cycle,
}

/// Resolve one nginx stream `pass <address>` content handler.
///
/// nginx parses the value as an address with an explicit port and re-dispatches
/// the connection to the listening socket bound there; that socket's default
/// virtual server (`default_server`, else the first server on the socket) runs
/// its own content handler. A chain of `pass` directives is followed up to
/// [`STREAM_PASS_MAX_PASSES`] jumps.
///
/// # Errors
///
/// Returns [`StreamPassError::InvalidAddress`] when the address is not a
/// literal IP/port, [`StreamPassError::NotConfigured`] when no listener binds
/// it, and [`StreamPassError::Cycle`] when the chain loops.
pub fn resolve_stream_pass_target<'a>(
    configs: &'a [StreamServerConfig],
    address: &str,
) -> Result<&'a StreamServerConfig, StreamPassError> {
    let socket = parse_pass_socket(address).ok_or(StreamPassError::InvalidAddress)?;
    let mut current = socket_target(configs, socket).ok_or(StreamPassError::NotConfigured)?;
    let mut visited = vec![current];
    let mut passes = 0_u8;
    while let Some(next_address) = configs[current].pass_address.as_deref() {
        passes = passes.saturating_add(1);
        if passes > STREAM_PASS_MAX_PASSES {
            return Err(StreamPassError::Cycle);
        }
        let next_socket = parse_pass_socket(next_address).ok_or(StreamPassError::InvalidAddress)?;
        let next = socket_target(configs, next_socket).ok_or(StreamPassError::NotConfigured)?;
        if visited.contains(&next) {
            return Err(StreamPassError::Cycle);
        }
        visited.push(next);
        current = next;
    }
    Ok(&configs[current])
}

/// Split a validated `pass` address into its IP/port pair.
fn parse_pass_socket(address: &str) -> Option<(IpAddr, u16)> {
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        rest.split_once("]:")?
    } else {
        let (host, port) = address.rsplit_once(':')?;
        if host.contains(':') {
            return None;
        }
        (host, port)
    };
    Some((host.parse().ok()?, port.parse().ok()?))
}

/// Whether one stream server listens on the `pass` socket.
///
/// nginx matches a literal listen address exactly and a wildcard listener by
/// socket family and port (`ngx_stream_pass_match`).
fn pass_target_matches((host, port): (IpAddr, u16), config: &StreamServerConfig) -> bool {
    if config.port != port {
        return false;
    }
    let Ok(bind) = config.bind.parse::<IpAddr>() else {
        return false;
    };
    if bind.is_unspecified() {
        return bind.is_ipv4() == host.is_ipv4();
    }
    bind == host
}

/// The default virtual server of the socket a `pass` address names.
fn socket_target(configs: &[StreamServerConfig], socket: (IpAddr, u16)) -> Option<usize> {
    let targets: Vec<usize> = configs
        .iter()
        .enumerate()
        .filter(|(_, config)| pass_target_matches(socket, config))
        .map(|(index, _)| index)
        .collect();
    targets
        .iter()
        .copied()
        .find(|index| configs[*index].default_server)
        .or_else(|| targets.first().copied())
}

/// Variables a stream `map` source may use (all known at accept/preread time).
pub const STREAM_MAP_SOURCES: &[&str] = &[
    "remote_addr",
    "remote_port",
    "server_addr",
    "server_port",
    "ssl_preread_server_name",
    // nginx stream GeoIP module variables (accept-time lookup).
    "geoip_country_code",
    "geoip_country_code3",
    "geoip_country_name",
    "geoip_org",
    "geoip_city_continent_code",
    "geoip_city_country_code",
    "geoip_city_country_code3",
    "geoip_city_country_name",
    "geoip_region",
    "geoip_region_name",
    "geoip_city",
    "geoip_postal_code",
    "geoip_latitude",
    "geoip_longitude",
    "geoip_dma_code",
    "geoip_area_code",
];

/// Variables a `set` / `return` template may reference.
pub const STREAM_TEMPLATE_VARIABLES: &[&str] = &[
    "remote_addr",
    "remote_port",
    "server_addr",
    "server_port",
    "binary_remote_addr",
    "ssl_preread_server_name",
    // nginx stream GeoIP module variables (accept-time lookup).
    "geoip_country_code",
    "geoip_country_code3",
    "geoip_country_name",
    "geoip_org",
    "geoip_city_continent_code",
    "geoip_city_country_code",
    "geoip_city_country_code3",
    "geoip_city_country_name",
    "geoip_region",
    "geoip_region_name",
    "geoip_city",
    "geoip_postal_code",
    "geoip_latitude",
    "geoip_longitude",
    "geoip_dma_code",
    "geoip_area_code",
];

/// Parse the `$name` references in a stream template in order of appearance.
/// `$$` is a literal dollar sign (nginx-style escapes are not supported by the
/// stream plane, so an unsupported form is reported by the caller).
#[must_use]
pub fn stream_template_variables(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(index) = rest.find('$') {
        let after = &rest[index + 1..];
        if let Some(literal) = after.strip_prefix('$') {
            rest = literal;
            continue;
        }
        let end = after
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(after.len());
        if end > 0 {
            names.push(after[..end].to_owned());
        }
        rest = &after[end..];
    }
    names
}

/// Expand a stream template against already-resolved variables. Unknown
/// references expand to the empty string; configuration validation rejects them
/// before a connection can run.
#[must_use]
pub fn expand_stream_template(template: &str, variables: &[(String, String)]) -> String {
    let mut output = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(index) = rest.find('$') {
        output.push_str(&rest[..index]);
        let after = &rest[index + 1..];
        if let Some(literal) = after.strip_prefix('$') {
            output.push('$');
            rest = literal;
            continue;
        }
        let end = after
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(after.len());
        let name = &after[..end];
        if let Some((_, value)) = variables
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == name)
        {
            output.push_str(value);
        }
        rest = &after[end..];
    }
    output.push_str(rest);
    output
}

/// Validate one stream `set` assignment name/value pair.
pub fn validate_stream_set(name: &str, value: &str, available: &[String]) -> Result<(), String> {
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || name.is_empty()
    {
        return Err(format!(
            "stream set variable `{name}` is not a valid nginx variable name"
        ));
    }
    if value.len() > 4096 {
        return Err("stream set value exceeds the 4096-byte bound".to_owned());
    }
    for reference in stream_template_variables(value) {
        if !is_known_stream_variable(&reference, available) {
            return Err(format!(
                "stream set `{name}` uses `${reference}`, which the stream plane does not know at accept/preread time; supported: $remote_addr $remote_port $server_addr $server_port $binary_remote_addr $ssl_preread_server_name, the stream GeoIP variables ($geoip_country_code … $geoip_area_code), plus declared stream map results and earlier set values"
            ));
        }
    }
    Ok(())
}

/// Validate one stream `return` value template.
pub fn validate_stream_return(value: &str, available: &[String]) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 {
        return Err("stream `return` value must be 1..4096 bytes".to_owned());
    }
    for reference in stream_template_variables(value) {
        if !is_known_stream_variable(&reference, available) {
            return Err(format!(
                "stream `return` uses `${reference}`, which the stream plane does not know at accept/preread time; supported: $remote_addr $remote_port $server_addr $server_port $binary_remote_addr $ssl_preread_server_name, the stream GeoIP variables ($geoip_country_code … $geoip_area_code), plus declared stream map results and set values"
            ));
        }
    }
    Ok(())
}

fn is_known_stream_variable(name: &str, available: &[String]) -> bool {
    STREAM_TEMPLATE_VARIABLES.contains(&name) || available.iter().any(|candidate| candidate == name)
}

/// Per-connection facts the stream variable engine resolves.
#[derive(Debug, Clone)]
pub struct StreamVariableFacts {
    pub remote_addr: IpAddr,
    pub remote_port: u16,
    pub server_addr: IpAddr,
    pub server_port: u16,
    pub ssl_preread_server_name: Option<String>,
    /// Resolved nginx stream GeoIP variables for the accepted connection.
    pub geoip: GeoIpRequestState,
}

impl MapVariables for StreamVariableFacts {
    fn map_variable(&self, name: &str) -> Option<String> {
        self.lookup(name)
    }
}

impl StreamVariableFacts {
    /// Resolve one known variable by name (without `$`).
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<String> {
        match name {
            "remote_addr" => Some(self.remote_addr.to_string()),
            "binary_remote_addr" | "remote_port" => Some(if name == "remote_port" {
                self.remote_port.to_string()
            } else {
                self.remote_addr.to_string()
            }),
            "server_addr" => Some(self.server_addr.to_string()),
            "server_port" => Some(self.server_port.to_string()),
            "ssl_preread_server_name" => {
                Some(self.ssl_preread_server_name.clone().unwrap_or_default())
            }
            _ => self.geoip.value(name).map(str::to_owned),
        }
    }
}

/// Resolve a stream `set` assignment sequence against the per-connection facts.
#[must_use]
pub fn evaluate_stream_sets(
    sets: &[StreamSetVariableConfig],
    facts: &StreamVariableFacts,
    seed: &[(String, String)],
) -> Vec<(String, String)> {
    let mut values: Vec<(String, String)> = Vec::new();
    // The known per-connection facts are the base layer every template sees.
    for name in STREAM_TEMPLATE_VARIABLES {
        if let Some(value) = facts.lookup(name) {
            values.push(((*name).to_owned(), value));
        }
    }
    // Map results and any caller-provided results override the facts.
    values.extend(seed.iter().cloned());
    for set in sets {
        let expanded = expand_stream_template(&set.value, &values);
        values.push((set.name.clone(), expanded));
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> StreamVariableFacts {
        StreamVariableFacts {
            remote_addr: "198.51.100.7".parse().expect("ip"),
            remote_port: 12345,
            server_addr: "127.0.0.1".parse().expect("ip"),
            server_port: 19000,
            ssl_preread_server_name: None,
            geoip: GeoIpRequestState {
                country_code: "US".to_owned(),
                city: "Testville".to_owned(),
                ..GeoIpRequestState::default()
            },
        }
    }

    #[test]
    fn resolves_geoip_variables_from_the_accepted_address() {
        let facts = facts();
        assert_eq!(facts.lookup("geoip_country_code").as_deref(), Some("US"));
        assert_eq!(facts.lookup("geoip_city").as_deref(), Some("Testville"));
        assert_eq!(facts.lookup("geoip_postal_code").as_deref(), Some(""));
        assert_eq!(facts.lookup("unknown"), None);
    }

    #[test]
    fn stream_set_templates_can_reference_geoip_variables() {
        let facts = facts();
        let sets = vec![StreamSetVariableConfig {
            name: "zone".to_owned(),
            value: "geo-$geoip_country_code".to_owned(),
        }];
        let values = evaluate_stream_sets(&sets, &facts, &[]);
        assert!(values.contains(&("zone".to_owned(), "geo-US".to_owned())));
        assert!(values.contains(&("geoip_country_code".to_owned(), "US".to_owned())));
    }
}
