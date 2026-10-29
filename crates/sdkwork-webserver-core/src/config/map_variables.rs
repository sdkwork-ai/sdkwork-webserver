//! Bounded nginx `map` execution (http-context variables).
//!
//! nginx `map $source $result { default …; <key> <value>; … }` declares a
//! result variable whose value is selected from an authored key/value table by
//! the current value of one source variable. This module owns the *pure*
//! parts — declaration validation, key matching, and template expansion —
//! while the data plane supplies the request facts ([`MapVariables`]).
//!
//! The subset is bounded and fail-closed:
//!
//! - the source must be exactly one supported variable: `$host`, `$scheme`,
//!   `$remote_addr`, `$server_port`, `$request_uri`, `$uri`, `$document_uri`,
//!   `$args`, `$is_args`, `$request_method`, `$server_name`, or
//!   `$http_<name>` / `$arg_<name>` / `$cookie_<name>`;
//! - values may be literals or mix in those same variables (expansion happens
//!   in map context, exactly like nginx);
//! - `default` may be omitted (the result is then the empty string);
//! - keys may be regular expressions (`~pattern`, `~*pattern`), checked in
//!   authored order after exact/wildcard keys, as nginx does; a value may
//!   reference the `$0`–`$9` capture groups of the matching regex;
//! - `include` is expanded by the nginx parser; the `volatile` parameter is
//!   not executed and fails closed.
//!
//! Key matching mirrors nginx 1.29.6 (differential-probed):
//!
//! - every key is matched case-insensitively (nginx lowercases the map hash);
//! - without `hostnames;` a key is an exact string, so a `*` in a key is a
//!   literal character;
//! - with `hostnames;` a key may be `*.suffix` (any non-empty subdomain
//!   prefix; the apex does **not** match), `prefix.*` (any non-empty suffix),
//!   or `.suffix` (the shorthand for the apex *and* any subdomain);
//! - an exact key wins over any wildcard; among leading wildcards the longest
//!   suffix wins; leading wildcards are consulted before trailing wildcards;
//!   among trailing wildcards the longest prefix wins; regular-expression keys
//!   are then evaluated in authored order (differential-confirmed against
//!   nginx 1.29.6, including `~` case sensitivity and `~*`); then `default`.

use regex::Regex;

use super::model::{MapConfig, MapEntryConfig};

/// Request variables a `map` source (and a map value template) may reference.
/// This is exactly the vocabulary the nginx import plane can evaluate in map
/// context; everything else fails closed at configuration load.
pub const MAP_SOURCE_VARIABLES: &[&str] = &[
    "host",
    "scheme",
    "remote_addr",
    "server_port",
    "request_uri",
    "uri",
    "document_uri",
    "args",
    "is_args",
    "request_method",
    "server_name",
];

/// Suffixes that introduce a per-request variable family (`$http_<name>`,
/// `$arg_<name>`, `$cookie_<name>`).
pub const MAP_SOURCE_PREFIXES: &[&str] = &["http_", "arg_", "cookie_"];

/// Maximum authored key length. nginx has no comparable bound; this keeps a
/// pathological configuration from materializing an unbounded hash key.
pub const MAX_MAP_KEY_BYTES: usize = 4096;
/// Maximum authored map value length (a value may be a variable template).
pub const MAX_MAP_VALUE_BYTES: usize = 4096;
/// Maximum number of entries in one `map` block.
pub const MAX_MAP_ENTRIES: usize = 4096;

/// Request facts a `map` declaration is evaluated against.
pub trait MapVariables {
    /// Resolve one variable by its name without the `$` prefix. `None` means
    /// the variable does not apply to this request (for example `$arg_x` when
    /// the argument is absent from a source *key* lookup — nginx distinguishes
    /// an absent header/argument/cookie from an empty one in neither case, so
    /// implementations return the empty string for those; `None` is reserved
    /// for a variable the implementation cannot answer at all).
    fn map_variable(&self, name: &str) -> Option<String>;
}

/// One validated key form.
#[derive(Debug, Clone)]
enum MapKey {
    /// Exact string, matched case-insensitively.
    Exact(String),
    /// `*.suffix`: any non-empty leading label sequence followed by `.suffix`.
    HeadWildcard(String),
    /// `prefix.*`: `prefix.` followed by a non-empty trailing label sequence.
    TailWildcard(String),
    /// `.suffix`: the apex `suffix` or any subdomain of it.
    DotSuffix(String),
    /// `~pattern` / `~*pattern`: an ordered regular expression key.
    Regex(Regex),
    /// `default` entry.
    Default,
}

impl MapKey {
    fn specificity(&self) -> usize {
        match self {
            Self::Exact(value) => value.len(),
            Self::HeadWildcard(value) | Self::DotSuffix(value) => value.len(),
            Self::TailWildcard(value) => value.len(),
            Self::Regex(_) => 0,
            Self::Default => 0,
        }
    }
}

/// Parse one authored key in the context of the map's `hostnames` flag.
///
/// Returns the matching form or a diagnostic. Wildcard forms are only valid
/// with `hostnames;`; an interior `*` is never valid in hostnames mode
/// (nginx: `invalid hostname or wildcard`).
fn parse_map_key(key: &str, hostnames: bool) -> Result<MapKey, String> {
    if key.len() > MAX_MAP_KEY_BYTES {
        return Err(format!(
            "map key exceeds the {MAX_MAP_KEY_BYTES}-byte bound"
        ));
    }
    if key == "default" {
        return Ok(MapKey::Default);
    }
    if let Some(pattern) = key.strip_prefix("~*") {
        let regex = Regex::new(&format!("(?i){pattern}"))
            .map_err(|error| format!("map regex key `{key}` is invalid: {error}"))?;
        return Ok(MapKey::Regex(regex));
    }
    if let Some(pattern) = key.strip_prefix('~') {
        let regex = Regex::new(pattern)
            .map_err(|error| format!("map regex key `{key}` is invalid: {error}"))?;
        return Ok(MapKey::Regex(regex));
    }
    let normalized = key.to_ascii_lowercase();
    if !hostnames {
        return Ok(MapKey::Exact(normalized));
    }
    if let Some(suffix) = normalized.strip_prefix("*.") {
        if suffix.is_empty() || suffix.contains('*') {
            return Err(format!(
                "map key `{key}` is not a valid host wildcard; use `*.suffix` or `prefix.*`"
            ));
        }
        return Ok(MapKey::HeadWildcard(suffix.to_owned()));
    }
    if let Some(prefix) = normalized.strip_suffix(".*") {
        if prefix.is_empty() || prefix.contains('*') {
            return Err(format!(
                "map key `{key}` is not a valid host wildcard; use `*.suffix` or `prefix.*`"
            ));
        }
        return Ok(MapKey::TailWildcard(prefix.to_owned()));
    }
    if let Some(suffix) = normalized.strip_prefix('.') {
        if suffix.is_empty() || suffix.contains('*') {
            return Err(format!(
                "map key `{key}` is not a valid host wildcard; use `*.suffix`, `prefix.*`, or `.suffix`"
            ));
        }
        return Ok(MapKey::DotSuffix(suffix.to_owned()));
    }
    if normalized.contains('*') {
        return Err(format!(
            "map key `{key}` is not a valid hostname or wildcard; use `*.suffix`, `prefix.*`, or an exact host"
        ));
    }
    Ok(MapKey::Exact(normalized))
}

fn matches(key: &MapKey, source: &str) -> bool {
    match key {
        MapKey::Exact(value) => source == value,
        MapKey::HeadWildcard(suffix) => {
            source.len() > suffix.len() + 1
                && source.ends_with(suffix)
                && source.as_bytes()[source.len() - suffix.len() - 1] == b'.'
        }
        MapKey::TailWildcard(prefix) => {
            source.len() > prefix.len() + 1
                && source.starts_with(prefix)
                && source.as_bytes()[prefix.len()] == b'.'
        }
        MapKey::DotSuffix(suffix) => {
            source == suffix
                || (source.len() > suffix.len() + 1
                    && source.ends_with(suffix)
                    && source.as_bytes()[source.len() - suffix.len() - 1] == b'.')
        }
        MapKey::Regex(regex) => regex.is_match(source),
        MapKey::Default => false,
    }
}

/// Resolve the authored value for `source_value` without expanding variables.
///
/// The caller is responsible for expanding the returned template against the
/// request facts ([`expand_map_template`]); a `map` declares its key order, so
/// the exact/wildcard precedence above is what makes the result deterministic.
#[must_use]
pub fn resolve_map_value<'a>(map: &'a MapConfig, source_value: &str) -> &'a str {
    let source = source_value.to_ascii_lowercase();
    let mut keys: Vec<(MapKey, &'a MapEntryConfig)> = Vec::with_capacity(map.entries.len());
    for entry in &map.entries {
        // Entries were validated at configuration load; a key that somehow
        // fails to parse here can never match instead of panicking.
        if let Ok(key) = parse_map_key(&entry.key, map.hostnames) {
            keys.push((key, entry));
        }
    }
    // 1. Exact key (case-insensitive); the first authored duplicate wins but
    //    duplicates are rejected at load, so there is at most one.
    if let Some((_, entry)) = keys
        .iter()
        .find(|(key, _)| matches!(key, MapKey::Exact(_)) && matches(key, &source))
    {
        return &entry.value;
    }
    if map.hostnames {
        // 2. Leading wildcards, longest suffix first.
        let mut head: Vec<&(MapKey, &MapEntryConfig)> = keys
            .iter()
            .filter(|(key, _)| matches!(key, MapKey::HeadWildcard(_) | MapKey::DotSuffix(_)))
            .collect();
        head.sort_by_key(|(key, _)| std::cmp::Reverse(key.specificity()));
        for (key, entry) in head {
            if matches(key, &source) {
                return &entry.value;
            }
        }
        // 3. Trailing wildcards, longest prefix first.
        let mut tail: Vec<&(MapKey, &MapEntryConfig)> = keys
            .iter()
            .filter(|(key, _)| matches!(key, MapKey::TailWildcard(_)))
            .collect();
        tail.sort_by_key(|(key, _)| std::cmp::Reverse(key.specificity()));
        for (key, entry) in tail {
            if matches(key, &source) {
                return &entry.value;
            }
        }
    }
    map.default.as_deref().unwrap_or("")
}

/// The authored value selected for one source value, plus the regular
/// expression capture groups when the match came from a `~`/`~*` key.
struct MapMatch<'a> {
    template: &'a str,
    captures: Option<Vec<Option<String>>>,
}

/// Resolve the authored value for `source_value`, including regex capture
/// groups. Order matches nginx: exact key, leading wildcards (longest first),
/// trailing wildcards (longest first), then regex keys in authored order.
fn resolve_map_match<'a>(map: &'a MapConfig, source_value: &str) -> MapMatch<'a> {
    let source = source_value.to_ascii_lowercase();
    let mut keys: Vec<(MapKey, &'a MapEntryConfig)> = Vec::with_capacity(map.entries.len());
    for entry in &map.entries {
        // Entries were validated at configuration load; a key that somehow
        // fails to parse here can never match instead of panicking.
        if let Ok(key) = parse_map_key(&entry.key, map.hostnames) {
            keys.push((key, entry));
        }
    }
    let wildkeys: Vec<&(MapKey, &'a MapEntryConfig)> = keys
        .iter()
        .filter(|(key, _)| !matches!(key, MapKey::Regex(_) | MapKey::Default))
        .collect();
    // 1. Exact key (case-insensitive); the first authored duplicate wins but
    //    duplicates are rejected at load, so there is at most one.
    if let Some((_, entry)) = wildkeys
        .iter()
        .find(|(key, _)| matches!(key, MapKey::Exact(_)) && matches(key, &source))
    {
        return MapMatch {
            template: &entry.value,
            captures: None,
        };
    }
    if map.hostnames {
        // 2. Leading wildcards, longest suffix first.
        let mut head: Vec<&(MapKey, &MapEntryConfig)> = wildkeys
            .iter()
            .copied()
            .filter(|(key, _)| matches!(key, MapKey::HeadWildcard(_) | MapKey::DotSuffix(_)))
            .collect();
        head.sort_by_key(|(key, _)| std::cmp::Reverse(key.specificity()));
        for (key, entry) in head {
            if matches(key, &source) {
                return MapMatch {
                    template: &entry.value,
                    captures: None,
                };
            }
        }
        // 3. Trailing wildcards, longest prefix first.
        let mut tail: Vec<&(MapKey, &MapEntryConfig)> = wildkeys
            .iter()
            .copied()
            .filter(|(key, _)| matches!(key, MapKey::TailWildcard(_)))
            .collect();
        tail.sort_by_key(|(key, _)| std::cmp::Reverse(key.specificity()));
        for (key, entry) in tail {
            if matches(key, &source) {
                return MapMatch {
                    template: &entry.value,
                    captures: None,
                };
            }
        }
    }
    // 4. Regex keys, authored order, first match wins.
    for (key, entry) in &keys {
        let MapKey::Regex(regex) = key else {
            continue;
        };
        if let Some(captures) = regex.captures(source_value) {
            let groups = (0..captures.len())
                .map(|index| captures.get(index).map(|group| group.as_str().to_owned()))
                .collect();
            return MapMatch {
                template: &entry.value,
                captures: Some(groups),
            };
        }
    }
    MapMatch {
        template: map.default.as_deref().unwrap_or(""),
        captures: None,
    }
}

/// Substitute `$0`–`$9` capture references in a map value template.
fn substitute_map_captures(template: &str, captures: &[Option<String>]) -> String {
    let mut output = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let character = template[index..]
                .chars()
                .next()
                .expect("index is on a character boundary");
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            output.push('$');
            index += 1;
            continue;
        }
        if index < bytes.len() && bytes[index].is_ascii_digit() {
            let group = (bytes[index] - b'0') as usize;
            output.push_str(
                captures
                    .get(group)
                    .and_then(|value| value.as_deref())
                    .unwrap_or(""),
            );
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        output.push('$');
        output.push_str(&template[start..index]);
    }
    output
}

/// Parse a `$name` reference into `name`.
pub fn parse_variable_reference(reference: &str) -> Result<&str, String> {
    let Some(name) = reference.strip_prefix('$') else {
        return Err(format!(
            "expected a `$variable` reference, found `{reference}`"
        ));
    };
    if name.is_empty() {
        return Err("bare `$` is not a variable reference".to_owned());
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!("`${name}` is not a valid nginx variable name"));
    }
    Ok(name)
}

/// Whether `name` is part of the executable map variable vocabulary.
#[must_use]
pub fn is_supported_map_variable(name: &str) -> bool {
    if MAP_SOURCE_VARIABLES.contains(&name) {
        return true;
    }
    MAP_SOURCE_PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty())
    })
}

/// Validate a map source: exactly one supported variable, no surrounding text.
pub fn validate_map_source(source: &str) -> Result<&str, String> {
    let name = parse_variable_reference(source)?;
    if !is_supported_map_variable(name) {
        return Err(format!(
            "map source `{source}` is not supported; use one of ${} or $http_<name>/$arg_<name>/$cookie_<name>",
            MAP_SOURCE_VARIABLES.join(" $")
        ));
    }
    Ok(name)
}

/// Validate the variables referenced by a map value template.
pub fn validate_map_value_template(template: &str) -> Result<(), String> {
    validate_map_value_template_with_captures(template, false)
}

/// [`validate_map_value_template`] with `$0`–`$9` capture references allowed
/// (only meaningful for a map that declares at least one regex key).
pub fn validate_map_value_template_with_captures(
    template: &str,
    allow_captures: bool,
) -> Result<(), String> {
    if template.len() > MAX_MAP_VALUE_BYTES {
        return Err(format!(
            "map value exceeds the {MAX_MAP_VALUE_BYTES}-byte bound"
        ));
    }
    let mut rest = template;
    while let Some(start) = rest.find('$') {
        let after = &rest[start + 1..];
        if let Some(escaped) = after.strip_prefix('$') {
            rest = escaped;
            continue;
        }
        let end = after
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(after.len());
        if end == 0 {
            return Err(format!(
                "map value `{template}` contains a bare `$`; write `$$` for a literal dollar sign"
            ));
        }
        let name = &after[..end];
        if allow_captures && name.len() == 1 && name.as_bytes()[0].is_ascii_digit() {
            rest = &after[end..];
            continue;
        }
        if !is_supported_map_variable(name) {
            return Err(format!(
                "map value `{template}` uses `${name}`, which the runtime does not expand in map context; supported: ${} and $http_<name>/$arg_<name>/$cookie_<name>",
                MAP_SOURCE_VARIABLES.join(" $")
            ));
        }
        rest = &after[end..];
    }
    if !template
        .bytes()
        .all(|byte| byte == b'\t' || (0x20..=0x7e).contains(&byte) || byte >= 0x80)
    {
        return Err(
            "map value must be printable text (HTTP field-value characters or UTF-8)".to_owned(),
        );
    }
    Ok(())
}

/// Expand `$variable` references in a map template. `$$` is a literal `$`.
pub fn expand_map_template(template: &str, variables: &dyn MapVariables) -> String {
    let mut output = String::with_capacity(template.len() + 16);
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let character = template[index..]
                .chars()
                .next()
                .expect("index is on a character boundary");
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            output.push('$');
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        if start == index {
            // A bare `$` reaches the output literally; configuration
            // validation rejects it, so this only protects unvalidated input.
            output.push('$');
            continue;
        }
        let name = &template[start..index];
        match variables.map_variable(name) {
            Some(value) => output.push_str(&value),
            None => {
                // Keep the reference literal rather than dropping it.
                output.push('$');
                output.push_str(name);
            }
        }
    }
    output
}

/// Evaluate one `map` declaration against the request facts.
#[must_use]
pub fn evaluate_map(map: &MapConfig, variables: &dyn MapVariables) -> String {
    let Ok(source_name) = validate_map_source(&map.source) else {
        return String::new();
    };
    let source_value = variables.map_variable(source_name).unwrap_or_default();
    let matched = resolve_map_match(map, &source_value);
    let template = match &matched.captures {
        Some(captures) => substitute_map_captures(matched.template, captures),
        None => matched.template.to_owned(),
    };
    expand_map_template(&template, variables)
}

/// Validate the executable subset of every `map` declaration and every
/// reference to a map result in the rest of the configuration.
///
/// Duplicate result names fail closed. nginx silently lets the last `map` win
/// (differential-confirmed against nginx 1.29.6), but a duplicate is almost
/// always an authoring mistake and a silently shadowed map is a security
/// hazard; the runtime refuses the configuration with a located diagnostic
/// instead.
pub fn validate_map_definitions(maps: &[MapConfig]) -> Result<(), String> {
    let mut results: Vec<&str> = Vec::with_capacity(maps.len());
    for map in maps {
        let result = parse_variable_reference(&map.result)
            .map_err(|message| format!("map result `{}` is invalid: {message}", map.result))?;
        if !result
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!(
                "map result `{}` is not a valid nginx variable name",
                map.result
            ));
        }
        if MAP_SOURCE_VARIABLES.contains(&result)
            || MAP_SOURCE_PREFIXES.iter().any(|prefix| {
                result
                    .strip_prefix(prefix)
                    .is_some_and(|rest| !rest.is_empty())
            })
        {
            return Err(format!(
                "map result `{}` collides with a built-in request variable; choose another name",
                map.result
            ));
        }
        if result == "request" || result == "document_root" || result == "realpath_root" {
            return Err(format!(
                "map result `{}` collides with a built-in request variable; choose another name",
                map.result
            ));
        }
        if results.contains(&result) {
            return Err(format!(
                "duplicate map result `{}`; each map result variable may be declared once (nginx would silently shadow it)",
                map.result
            ));
        }
        results.push(result);

        validate_map_source(&map.source)?;
        // A map with at least one regex key may write `$0`–`$9` capture
        // references into its values (nginx expands them from the matching
        // regex); capture references in a map without regex keys fail closed.
        let has_regex_key = map.entries.iter().any(|entry| entry.key.starts_with('~'));
        for entry in &map.entries {
            if entry.key.len() > MAX_MAP_KEY_BYTES {
                return Err(format!(
                    "map `{}` key exceeds the {MAX_MAP_KEY_BYTES}-byte bound",
                    map.result
                ));
            }
            parse_map_key(&entry.key, map.hostnames).map_err(|message| {
                format!(
                    "map `{}` key `{}` is invalid: {message}",
                    map.result, entry.key
                )
            })?;
            validate_map_value_template_with_captures(&entry.value, has_regex_key).map_err(
                |message| {
                    format!(
                        "map `{}` value `{}` is invalid: {message}",
                        map.result, entry.value
                    )
                },
            )?;
        }
        if let Some(default) = &map.default {
            validate_map_value_template_with_captures(default, has_regex_key).map_err(
                |message| {
                    format!(
                        "map `{}` default `{default}` is invalid: {message}",
                        map.result
                    )
                },
            )?;
        }
        if map.entries.len() > MAX_MAP_ENTRIES {
            return Err(format!(
                "map `{}` has {} entries; the runtime executes at most {MAX_MAP_ENTRIES}",
                map.result,
                map.entries.len()
            ));
        }
        // Duplicate authored keys are a configuration error in nginx too
        // (`conflicting parameter`), including the `.suffix` / `*.suffix`
        // shorthand collision; reject them deterministically.
        let mut seen: Vec<(MapKey, &str)> = Vec::new();
        for entry in &map.entries {
            let key = parse_map_key(&entry.key, map.hostnames)?;
            if matches!(key, MapKey::Regex(_)) {
                // nginx keeps every regex key in authored order; duplicate
                // patterns are legal (the first one wins at request time).
                continue;
            }
            if matches!(key, MapKey::Default) {
                return Err(format!(
                    "map `{}` declares key `default`; use the `default <value>;` form",
                    map.result
                ));
            }
            if seen.iter().any(|(existing, _)| same_key(existing, &key)) {
                return Err(format!(
                    "map `{}` declares duplicate key `{}`; nginx rejects conflicting map parameters",
                    map.result, entry.key
                ));
            }
            seen.push((key, &entry.key));
        }
        if map.entries.is_empty() && map.default.is_none() {
            return Err(format!(
                "map `{}` declares no entries and no default; it can only ever expand to the empty string",
                map.result
            ));
        }
    }
    Ok(())
}

/// Keys that collide in nginx's wildcard hash. `.suffix` is the shorthand for
/// `suffix` plus `*.suffix`, so it conflicts with either spelling (probed
/// against nginx 1.29.6, which reports `conflicting parameter` for the pair).
fn same_key(left: &MapKey, right: &MapKey) -> bool {
    match (left, right) {
        (MapKey::Exact(a), MapKey::Exact(b)) => a == b,
        (MapKey::HeadWildcard(a), MapKey::HeadWildcard(b)) => a == b,
        (MapKey::TailWildcard(a), MapKey::TailWildcard(b)) => a == b,
        (MapKey::DotSuffix(a), MapKey::DotSuffix(b)) => a == b,
        (MapKey::DotSuffix(suffix), MapKey::Exact(exact))
        | (MapKey::Exact(exact), MapKey::DotSuffix(suffix)) => suffix == exact,
        (MapKey::DotSuffix(suffix), MapKey::HeadWildcard(head))
        | (MapKey::HeadWildcard(head), MapKey::DotSuffix(suffix)) => suffix == head,
        _ => false,
    }
}

/// Name of a map result without its `$`, as used in expansion lookups.
#[must_use]
pub fn map_result_name(map: &MapConfig) -> Option<&str> {
    map.result.strip_prefix('$')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Facts(HashMap<String, String>);

    impl Facts {
        fn new(pairs: &[(&str, &str)]) -> Self {
            Self(
                pairs
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
            )
        }
    }

    impl MapVariables for Facts {
        fn map_variable(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    fn map_with(hostnames: bool, default: Option<&str>, entries: &[(&str, &str)]) -> MapConfig {
        MapConfig {
            source: "$host".to_owned(),
            result: "$mapped".to_owned(),
            hostnames,
            default: default.map(str::to_owned),
            entries: entries
                .iter()
                .map(|(key, value)| MapEntryConfig {
                    key: (*key).to_owned(),
                    value: (*value).to_owned(),
                })
                .collect(),
        }
    }

    fn lookup(map: &MapConfig, source: &str) -> String {
        let facts = Facts::new(&[("host", source)]);
        evaluate_map(map, &facts)
    }

    #[test]
    fn exact_keys_are_case_insensitive_and_empty_matches_empty() {
        let map = map_with(false, None, &[("", "empty"), ("Foo", "bar")]);
        assert_eq!(lookup(&map, ""), "empty");
        assert_eq!(lookup(&map, "foo"), "bar");
        assert_eq!(lookup(&map, "FOO"), "bar");
        // No default: the result is the empty string, like nginx.
        assert_eq!(lookup(&map, "other"), "");
    }

    #[test]
    fn default_is_used_when_no_key_matches() {
        let map = map_with(false, Some("fallback"), &[("a", "A")]);
        assert_eq!(lookup(&map, "a"), "A");
        assert_eq!(lookup(&map, "b"), "fallback");
    }

    #[test]
    fn hostnames_head_wildcard_matches_subdomains_but_not_the_apex() {
        let map = map_with(true, Some("d"), &[("*.example.com", "star")]);
        assert_eq!(lookup(&map, "a.example.com"), "star");
        assert_eq!(lookup(&map, "a.b.example.com"), "star");
        assert_eq!(lookup(&map, "EXAMPLE.COM"), "d");
        assert_eq!(lookup(&map, "example.com"), "d");
    }

    #[test]
    fn hostnames_tail_wildcard_matches_a_non_empty_suffix() {
        let map = map_with(true, Some("d"), &[("www.prefix.*", "pref")]);
        assert_eq!(lookup(&map, "www.prefix.org"), "pref");
        assert_eq!(lookup(&map, "WWW.PREFIX.ORG"), "pref");
        assert_eq!(lookup(&map, "www.prefix."), "d");
    }

    #[test]
    fn hostnames_dot_suffix_matches_the_apex_and_subdomains() {
        let map = map_with(true, Some("d"), &[(".example.org", "dot")]);
        assert_eq!(lookup(&map, "example.org"), "dot");
        assert_eq!(lookup(&map, "x.example.org"), "dot");
        assert_eq!(lookup(&map, "other.org"), "d");
    }

    #[test]
    fn hostname_precedence_is_exact_then_longest_head_then_longest_tail() {
        let map = map_with(
            true,
            Some("d"),
            &[
                ("*.com", "head-com"),
                ("www.*", "tail-www"),
                ("*.x.com", "head-x"),
                ("www.x.com.*", "tail-www-x"),
                ("exact.com", "exact"),
            ],
        );
        // `www.com` matches a leading and a trailing wildcard: leading wins.
        assert_eq!(lookup(&map, "www.com"), "head-com");
        // Longer leading suffix wins.
        assert_eq!(lookup(&map, "a.x.com"), "head-x");
        // Longer trailing prefix wins.
        assert_eq!(lookup(&map, "www.x.com.y"), "tail-www-x");
        // An exact key always wins.
        assert_eq!(lookup(&map, "exact.com"), "exact");
    }

    #[test]
    fn wildcard_characters_are_literal_without_hostnames() {
        let map = map_with(false, None, &[("foo*", "literal")]);
        assert_eq!(lookup(&map, "foo*"), "literal");
        assert_eq!(lookup(&map, "foobar"), "");
    }

    #[test]
    fn map_values_expand_request_variables() {
        let map = MapConfig {
            source: "$host".to_owned(),
            result: "$mapped".to_owned(),
            hostnames: false,
            default: Some("default-$host-$request_method".to_owned()),
            entries: vec![MapEntryConfig {
                key: "a".to_owned(),
                value: "v:$$literal:$request_method".to_owned(),
            }],
        };
        let facts = Facts::new(&[("host", "a"), ("request_method", "GET")]);
        assert_eq!(evaluate_map(&map, &facts), "v:$literal:GET");
        let facts = Facts::new(&[("host", "other"), ("request_method", "POST")]);
        assert_eq!(evaluate_map(&map, &facts), "default-other-POST");
    }

    #[test]
    fn validation_rejects_unsupported_sources_and_values() {
        let mut map = map_with(false, None, &[("a", "A")]);
        map.source = "$upstream_addr".to_owned();
        assert!(validate_map_definitions(&[map.clone()]).is_err());

        map.source = "$host".to_owned();
        map.entries = vec![MapEntryConfig {
            key: "a".to_owned(),
            value: "$upstream_addr".to_owned(),
        }];
        assert!(validate_map_definitions(&[map.clone()]).is_err());

        map.entries = vec![MapEntryConfig {
            key: "a".to_owned(),
            value: "ok".to_owned(),
        }];
        map.result = "$host".to_owned();
        assert!(validate_map_definitions(&[map]).is_err());
    }

    #[test]
    fn validation_rejects_duplicate_results_and_keys() {
        let first = map_with(false, None, &[("a", "A")]);
        let mut second = first.clone();
        second.source = "$uri".to_owned();
        assert!(validate_map_definitions(&[first.clone(), second]).is_err());

        let mut duplicate_key = map_with(false, None, &[("a", "A"), ("A", "B")]);
        duplicate_key.entries[1].key = "a".to_owned();
        assert!(validate_map_definitions(&[duplicate_key]).is_err());

        let dot_and_star = map_with(
            true,
            None,
            &[(".example.org", "dot"), ("*.example.org", "star")],
        );
        assert!(validate_map_definitions(&[dot_and_star]).is_err());
    }

    #[test]
    fn regex_keys_execute_in_authored_order_after_hash_keys() {
        let map = map_with(
            false,
            Some("d"),
            &[("~^bar", "FIRST"), ("~^ba", "SECOND"), ("~*^BAR", "THIRD")],
        );
        assert_eq!(lookup(&map, "bar"), "FIRST");
        assert_eq!(lookup(&map, "baz"), "SECOND");
        // `~` is case-sensitive, so BAR falls through to `~*^BAR`.
        assert_eq!(lookup(&map, "BAR"), "THIRD");
        // An exact key wins over a regex.
        let map = map_with(false, None, &[("~^ex", "REGEX"), ("exact", "EXACT")]);
        assert_eq!(lookup(&map, "exact"), "EXACT");
    }

    #[test]
    fn regex_capture_groups_expand_in_map_values() {
        let map = map_with(false, Some("d"), &[("~^abc(?<n>[0-9]+)", "N-$1")]);
        assert_eq!(lookup(&map, "abc123"), "N-123");
        assert_eq!(lookup(&map, "zzz"), "d");
    }

    #[test]
    fn validation_rejects_mid_wildcards_and_bad_regexes() {
        let mid = map_with(true, None, &[("a*c", "R")]);
        assert!(validate_map_definitions(&[mid]).is_err());
        let bad = map_with(false, None, &[("~(", "R")]);
        assert!(validate_map_definitions(&[bad]).is_err());
    }

    #[test]
    fn capture_references_fail_closed_without_a_regex_key() {
        let map = map_with(false, None, &[("a", "v-$1")]);
        assert!(validate_map_definitions(&[map]).is_err());
    }
}
