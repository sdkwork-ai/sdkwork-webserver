//! Local file resolution source (`/etc/hosts` style), the deployment seed
//! surface: `sdkwork-deployments` exports its domain/IP inventory into this
//! file, and the data plane parses it into the memory layer at startup.

use std::{collections::HashMap, path::Path};

/// Parse `/etc/hosts`-style content: `IP hostname [alias ...]` lines with
/// `#` comments. Returns domain → addresses.
pub fn parse_hosts_file(content: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let Some(address) = tokens.next() else {
            continue;
        };
        for name in tokens {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            let entry = map.entry(name).or_default();
            if !entry.iter().any(|existing| existing == address) {
                entry.push(address.to_owned());
            }
        }
    }
    map
}

/// Load and parse a hosts-style file, returning the domain table.
pub fn load_hosts_file(path: &Path) -> Result<HashMap<String, Vec<String>>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read resolution file {}: {error}", path.display()))?;
    Ok(parse_hosts_file(&content))
}

/// Local file source: an immutable table of domain → addresses loaded at
/// startup from the configured file (deployment seed).
pub struct FileResolverSource {
    entries: HashMap<String, Vec<String>>,
}

impl FileResolverSource {
    pub fn load(path: &Path) -> Result<Self, String> {
        Ok(Self {
            entries: load_hosts_file(path)?,
        })
    }

    pub fn from_entries(entries: HashMap<String, Vec<String>>) -> Self {
        Self { entries }
    }

    /// Iterate the seeded entries (used to warm the memory layer).
    pub fn entries(&self) -> &HashMap<String, Vec<String>> {
        &self.entries
    }

    pub fn lookup(&self, domain: &str) -> Option<Vec<String>> {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        self.entries.get(&domain).cloned()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_style_content() {
        let content = "\
# comment
127.0.0.1 localhost api.local
10.0.0.5   gateway.internal
::1 localhost
";
        let map = parse_hosts_file(content);
        assert_eq!(
            map.get("localhost").map(|v| v.as_slice()),
            Some(&["127.0.0.1".to_owned(), "::1".to_owned()][..])
        );
        assert_eq!(
            map.get("api.local").map(|v| v.as_slice()),
            Some(&["127.0.0.1".to_owned()][..])
        );
        assert_eq!(
            map.get("gateway.internal").map(|v| v.as_slice()),
            Some(&["10.0.0.5".to_owned()][..])
        );
    }

    #[test]
    fn normalizes_case_and_trailing_dots() {
        let map = parse_hosts_file("10.0.0.9 API.Example.COM. alias.local");
        assert_eq!(
            map.get("api.example.com").map(|v| v.as_slice()),
            Some(&["10.0.0.9".to_owned()][..])
        );
        assert_eq!(
            map.get("alias.local").map(|v| v.as_slice()),
            Some(&["10.0.0.9".to_owned()][..])
        );
    }
}
