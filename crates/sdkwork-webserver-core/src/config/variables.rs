//! Bounded nginx variable-template expansion and validation.
//!
//! The nginx rewrite phase (`set` values, `if` operands, variable `proxy_pass`
//! targets, dynamic `root` values) uses the same template grammar:
//!
//! - `$name` and `${name}` expand a variable;
//! - `$1` .. `$9` expand a regex capture from the most recent location/`if`
//!   match (an absent capture is the empty string);
//! - `$$` is a literal `$`;
//! - every other character is literal text.
//!
//! The engine is intentionally small: no functions, no escapes beyond `$$`, and
//! the caller decides the legal variable names. Anything outside that subset is
//! a located configuration error at materialization, never a silent empty
//! expansion at request time.

/// Longest template accepted in any rewrite-phase value.
pub const MAX_TEMPLATE_BYTES: usize = 8 * 1024;

/// One variable reference found in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateVariable {
    /// Variable name without the `$`/`{}` delimiters, e.g. `env`, `http_host`,
    /// `1`.
    pub name: String,
    /// Byte range of the reference in the source template (delimiters
    /// included), for located diagnostics.
    pub start: usize,
    pub end: usize,
}

/// Parse the variable references in a template.
///
/// Returns an error for a bare trailing `$`, an empty `${}`, or an unclosed
/// `${`.
pub fn template_variables(template: &str) -> Result<Vec<TemplateVariable>, String> {
    if template.len() > MAX_TEMPLATE_BYTES {
        return Err(format!(
            "template is {} bytes; maximum is {MAX_TEMPLATE_BYTES}",
            template.len()
        ));
    }
    let bytes = template.as_bytes();
    let mut variables = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            index += 1;
            continue;
        }
        if index < bytes.len() && bytes[index] == b'{' {
            let Some(close) = template[index + 1..].find('}') else {
                return Err(format!(
                    "template `{template}` has an unclosed `${{` at byte {start}"
                ));
            };
            let name = &template[index + 1..index + 1 + close];
            if name.is_empty() {
                return Err(format!("template `{template}` has an empty `${{}}`"));
            }
            variables.push(TemplateVariable {
                name: name.to_owned(),
                start,
                end: index + 1 + close + 1,
            });
            index = index + 1 + close + 1;
            continue;
        }
        let name_start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        if name_start == index {
            return Err(format!(
                "template `{template}` has a bare `$` at byte {start}"
            ));
        }
        variables.push(TemplateVariable {
            name: template[name_start..index].to_owned(),
            start,
            end: index,
        });
    }
    Ok(variables)
}

/// Whether `name` is a regex capture reference (`1` .. `9`).
pub fn is_capture_variable(name: &str) -> bool {
    matches!(name, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

/// Expand a template with `lookup`; an unknown variable expands to the empty
/// string, exactly like nginx's "uninitialised variable is empty" rule (all
/// references were validated at materialization).
pub fn expand_template(template: &str, lookup: &mut dyn FnMut(&str) -> Option<String>) -> String {
    let bytes = template.as_bytes();
    let mut output = String::with_capacity(template.len() + 16);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'$' {
                index += 1;
            }
            output.push_str(&template[start..index]);
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            output.push('$');
            index += 1;
            continue;
        }
        if index < bytes.len() && bytes[index] == b'{' {
            let Some(close) = template[index + 1..].find('}') else {
                // Materialization rejects this; at request time the rest is
                // literal rather than a panic.
                output.push_str(&template[index - 1..]);
                break;
            };
            let name = &template[index + 1..index + 1 + close];
            if let Some(value) = lookup(name) {
                output.push_str(&value);
            }
            index = index + 1 + close + 1;
            continue;
        }
        let name_start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        if name_start == index {
            output.push('$');
            continue;
        }
        if let Some(value) = lookup(&template[name_start..index]) {
            output.push_str(&value);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_braced_variables() {
        let variables =
            template_variables("http://api_servers_${env}/$1.txt?x=$host").expect("parse");
        assert_eq!(
            variables
                .iter()
                .map(|variable| variable.name.as_str())
                .collect::<Vec<_>>(),
            vec!["env", "1", "host"]
        );
        assert!(template_variables("$").is_err());
        assert!(template_variables("${}").is_err());
        assert!(template_variables("${unclosed").is_err());
    }

    #[test]
    fn expands_plain_and_braced_variables_and_escapes() {
        let mut lookup = |name: &str| match name {
            "env" => Some("prod".to_owned()),
            "1" => Some("file".to_owned()),
            "host" => Some("api.example.com".to_owned()),
            _ => None,
        };
        assert_eq!(
            expand_template(
                "http://api_${env}/${1}.txt?h=$host&m=$missing&d=$$",
                &mut lookup
            ),
            "http://api_prod/file.txt?h=api.example.com&m=&d=$"
        );
    }

    #[test]
    fn rejects_oversized_templates() {
        let oversized = "a".repeat(MAX_TEMPLATE_BYTES + 1);
        assert!(template_variables(&oversized).is_err());
    }
}
