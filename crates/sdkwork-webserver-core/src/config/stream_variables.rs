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

use std::net::IpAddr;

use super::map_variables::MapVariables;
use super::model::StreamSetVariableConfig;

/// Variables a stream `map` source may use (all known at accept/preread time).
pub const STREAM_MAP_SOURCES: &[&str] = &[
    "remote_addr",
    "remote_port",
    "server_addr",
    "server_port",
    "ssl_preread_server_name",
];

/// Variables a `set` / `return` template may reference.
pub const STREAM_TEMPLATE_VARIABLES: &[&str] = &[
    "remote_addr",
    "remote_port",
    "server_addr",
    "server_port",
    "binary_remote_addr",
    "ssl_preread_server_name",
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
                "stream set `{name}` uses `${reference}`, which the stream plane does not know at accept/preread time; supported: $remote_addr $remote_port $server_addr $server_port $binary_remote_addr $ssl_preread_server_name plus declared stream map results and earlier set values"
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
                "stream `return` uses `${reference}`, which the stream plane does not know at accept/preread time; supported: $remote_addr $remote_port $server_addr $server_port $binary_remote_addr $ssl_preread_server_name plus declared stream map results and set values"
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
            _ => None,
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
