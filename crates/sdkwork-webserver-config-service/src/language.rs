//! Editor language mapping for configuration files.
//!
//! The admin UI renders configuration content inside a Monaco editor; the
//! catalog carries the Monaco language id so the surface stays declarative.
//! Monaco has no first-class TOML or nginx grammars, so the closest safe
//! grammar is selected (`ini`).

/// Return the Monaco language id for a configuration file name.
pub fn config_language_for(file_name: &str) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    let extension = lower.rsplit('.').next().unwrap_or("");
    match extension {
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" | "ini" | "conf" | "cfg" => "ini",
        "sh" | "env" => "shell",
        "md" => "markdown",
        _ => "plaintext",
    }
}

#[cfg(test)]
mod tests {
    use super::config_language_for;

    #[test]
    fn maps_known_config_extensions() {
        assert_eq!(config_language_for("config.toml"), "ini");
        assert_eq!(config_language_for("import.conf"), "ini");
        assert_eq!(config_language_for("layout-imports.toml"), "ini");
        assert_eq!(config_language_for("sdkwork.webserver.config.json"), "json");
        assert_eq!(config_language_for("sidecar.yaml"), "yaml");
        assert_eq!(config_language_for("development.env"), "shell");
        assert_eq!(config_language_for("unknown.xyz"), "plaintext");
    }
}
