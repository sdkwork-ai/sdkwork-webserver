//! Bounded nginx rewrite-phase executor (`set` / `if` / `rewrite` / `return` /
//! conditional `add_header`).
//!
//! nginx runs the rewrite phase in this order: server-level instructions
//! before location selection, then the selected location's instructions (and
//! exactly one `rewrite`/`return` may terminate it). The data plane executes
//! the materialized lists in authored order; regex matches populate `$1..$9`
//! used by later `set`/`rewrite` instructions in the same phase.

use axum::{
    body::Body,
    http::{header::LOCATION, HeaderValue, Response, StatusCode, Version},
};
use regex::Regex;
use sdkwork_webserver_core::{
    CustomHeaderConfig, IfOperatorConfig, RewritePhaseOpConfig, RouteConfig, RoutePathType,
};

use super::request_variables::RequestVariables;

/// Result of executing one rewrite-phase instruction list.
#[derive(Debug)]
pub(crate) enum PhaseControl {
    /// Continue with the current path (variables and headers may have changed).
    Continue,
    /// The phase changed the request path. `reselect` re-runs location
    /// selection (nginx `rewrite … last`); otherwise the current location
    /// keeps handling the request (`break`).
    Rewrite {
        path: String,
        query: Option<String>,
        reselect: bool,
    },
    /// The phase answered the request (`return` / external `rewrite`).
    Respond(Response<Body>),
}

/// The regex captures of the location match that selected `route`.
///
/// nginx exposes the location regex captures to the location rewrite phase;
/// `$0` is not addressable, matching the model's `$1..$9` vocabulary.
pub(crate) fn location_captures(route: &RouteConfig, path: &str) -> Vec<Option<String>> {
    let pattern = route.route_match.path.as_str();
    let compiled = match route.route_match.path_type {
        RoutePathType::Regex => pattern.to_owned(),
        RoutePathType::RegexIgnoreCase => format!("(?i){pattern}"),
        _ => return Vec::new(),
    };
    let Ok(regex) = Regex::new(&compiled) else {
        return Vec::new();
    };
    regex
        .captures(path)
        .map(|captures| {
            (0..captures.len())
                .map(|index| captures.get(index).map(|m| m.as_str().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Execute an ordered rewrite-phase instruction list.
pub(crate) fn execute(
    operations: &[RewritePhaseOpConfig],
    variables: &mut RequestVariables,
    headers: &mut Vec<CustomHeaderConfig>,
    version: Version,
) -> PhaseControl {
    for operation in operations {
        match operation {
            RewritePhaseOpConfig::Set { variable, value } => {
                let expanded = variables.expand(value);
                variables.set(variable, expanded);
            }
            RewritePhaseOpConfig::If { condition, body } => {
                if if_matches(condition, variables) {
                    match execute(body, variables, headers, version) {
                        PhaseControl::Continue => {}
                        other => return other,
                    }
                }
            }
            RewritePhaseOpConfig::AddHeader {
                name,
                value,
                always,
            } => {
                headers.push(CustomHeaderConfig {
                    name: name.clone(),
                    value: variables.expand(value),
                    always: *always,
                });
            }
            RewritePhaseOpConfig::Return {
                status,
                body,
                location,
            } => {
                let Some(status) = StatusCode::from_u16(*status).ok() else {
                    return PhaseControl::Respond(internal_error(version));
                };
                let mut response = Response::builder().status(status);
                if let Some(location) = location {
                    let expanded = variables.expand(location);
                    match HeaderValue::from_str(&expanded) {
                        Ok(value) => {
                            response = response.header(LOCATION, value);
                        }
                        Err(_) => return PhaseControl::Respond(internal_error(version)),
                    }
                }
                let payload = body.clone().unwrap_or_default();
                let body = if status.is_redirection() || payload.is_empty() {
                    Body::empty()
                } else {
                    Body::from(payload)
                };
                return PhaseControl::Respond(
                    response
                        .body(body)
                        .unwrap_or_else(|_| internal_error(version)),
                );
            }
            RewritePhaseOpConfig::Rewrite {
                pattern,
                replacement,
                flag,
            } => {
                let Ok(regex) = Regex::new(pattern) else {
                    return PhaseControl::Respond(internal_error(version));
                };
                let uri = match variables.query.as_deref() {
                    Some(query) if !query.is_empty() => {
                        format!("{}?{query}", variables.path)
                    }
                    _ => variables.path.clone(),
                };
                let Some(captures) = regex.captures(&uri) else {
                    variables.clear_captures();
                    continue;
                };
                variables.set_captures(
                    (0..captures.len())
                        .map(|index| captures.get(index).map(|m| m.as_str().to_owned())),
                );
                let expanded = variables.expand(replacement);
                let (next_path, next_query) = match expanded.split_once('?') {
                    Some((path, query)) => (path.to_owned(), Some(query.to_owned())),
                    None => (expanded, None),
                };
                return match flag {
                    sdkwork_webserver_core::RewriteFlag::Redirect => {
                        PhaseControl::Respond(redirect(StatusCode::FOUND, &next_path, version))
                    }
                    sdkwork_webserver_core::RewriteFlag::Permanent => PhaseControl::Respond(
                        redirect(StatusCode::MOVED_PERMANENTLY, &next_path, version),
                    ),
                    sdkwork_webserver_core::RewriteFlag::Last => PhaseControl::Rewrite {
                        path: next_path,
                        query: next_query,
                        reselect: true,
                    },
                    sdkwork_webserver_core::RewriteFlag::Break => PhaseControl::Rewrite {
                        path: next_path,
                        query: next_query,
                        reselect: false,
                    },
                };
            }
        }
    }
    PhaseControl::Continue
}

fn if_matches(
    condition: &sdkwork_webserver_core::IfConditionConfig,
    variables: &mut RequestVariables,
) -> bool {
    let left = variables.lookup(&condition.variable).unwrap_or_default();
    match condition.operator {
        IfOperatorConfig::Truthy => !left.is_empty(),
        IfOperatorConfig::NegatedTruthy => left.is_empty(),
        IfOperatorConfig::Equals | IfOperatorConfig::NotEquals => {
            let right = condition
                .value
                .as_deref()
                .map(|value| variables.expand(value))
                .unwrap_or_default();
            let equal = left == right;
            if condition.operator == IfOperatorConfig::Equals {
                equal
            } else {
                !equal
            }
        }
        IfOperatorConfig::Regex
        | IfOperatorConfig::NotRegex
        | IfOperatorConfig::RegexIgnoreCase
        | IfOperatorConfig::NotRegexIgnoreCase => {
            let ignore_case = matches!(
                condition.operator,
                IfOperatorConfig::RegexIgnoreCase | IfOperatorConfig::NotRegexIgnoreCase
            );
            let raw = condition.value.as_deref().unwrap_or("");
            let pattern = if ignore_case {
                format!("(?i){raw}")
            } else {
                raw.to_owned()
            };
            let negated = matches!(
                condition.operator,
                IfOperatorConfig::NotRegex | IfOperatorConfig::NotRegexIgnoreCase
            );
            match Regex::new(&pattern) {
                Ok(regex) => match regex.captures(&left) {
                    Some(captures) => {
                        variables.set_captures(
                            (0..captures.len())
                                .map(|index| captures.get(index).map(|m| m.as_str().to_owned())),
                        );
                        !negated
                    }
                    None => {
                        // A failed regex match clears the captures, like nginx.
                        variables.clear_captures();
                        negated
                    }
                },
                Err(_) => {
                    variables.clear_captures();
                    false
                }
            }
        }
    }
}

/// Apply rewrite-phase `add_header` entries to a response.
///
/// nginx emits `add_header` for 2xx/3xx/204/304 unless `always` is set.
pub(crate) fn apply_headers(
    mut response: Response<Body>,
    headers: &[CustomHeaderConfig],
) -> Response<Body> {
    let status = response.status();
    let default_visible = status.is_success()
        || status.is_redirection()
        || status == StatusCode::NO_CONTENT
        || status == StatusCode::NOT_MODIFIED;
    for header in headers {
        if !header.always && !default_visible {
            continue;
        }
        let Ok(name) = axum::http::HeaderName::from_bytes(header.name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(&header.value) else {
            continue;
        };
        response.headers_mut().insert(name, value);
    }
    response
}

fn redirect(status: StatusCode, location: &str, version: Version) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap_or_else(|_| internal_error(version));
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(LOCATION, value);
    }
    response
}

fn internal_error(version: Version) -> Response<Body> {
    let _ = version;
    let mut response = Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from("internal rewrite-phase error\n"))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, Method};
    use sdkwork_webserver_core::IfConditionConfig;

    fn variables(path: &str) -> RequestVariables {
        RequestVariables::new(
            &HeaderMap::new(),
            &Method::GET,
            "https",
            "api.example.com",
            "127.0.0.1".parse().expect("ip"),
            443,
            path,
            path,
            None,
            "api.example.com",
            "HTTP/1.1",
        )
    }

    fn run(
        operations: &[RewritePhaseOpConfig],
        variables: &mut RequestVariables,
    ) -> (PhaseControl, Vec<CustomHeaderConfig>) {
        let mut headers = Vec::new();
        let control = execute(operations, variables, &mut headers, Version::HTTP_11);
        (control, headers)
    }

    fn set(variable: &str, value: &str) -> RewritePhaseOpConfig {
        RewritePhaseOpConfig::Set {
            variable: variable.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn set_expands_previously_assigned_variables() {
        let mut variables = variables("/");
        variables.set("env", "prod".to_owned());
        let (control, _) = run(
            &[
                set("root_path", "/srv/$env/site"),
                set("target", "http://${env}.internal"),
            ],
            &mut variables,
        );
        assert!(matches!(control, PhaseControl::Continue));
        assert_eq!(
            variables.lookup("root_path").as_deref(),
            Some("/srv/prod/site")
        );
        assert_eq!(
            variables.lookup("target").as_deref(),
            Some("http://prod.internal")
        );
    }

    #[test]
    fn if_regex_captures_feed_the_body() {
        let mut variables = variables("/");
        let operations = vec![RewritePhaseOpConfig::If {
            condition: IfConditionConfig {
                variable: "host".to_owned(),
                operator: IfOperatorConfig::RegexIgnoreCase,
                value: Some("^([a-z]+)-(dev|prod)\\.example\\.com$".to_owned()),
            },
            body: vec![set("env", "$2")],
        }];
        let (_, _) = run(&operations, &mut variables);
        // The host in the test fixture has no environment suffix: no match, no
        // assignment, captures cleared.
        assert_eq!(variables.lookup("env"), None);
        assert_eq!(variables.expand("$1"), "");

        variables.authority = "admin-prod.example.com".to_owned();
        let (_, _) = run(&operations, &mut variables);
        assert_eq!(variables.lookup("env").as_deref(), Some("prod"));
        assert_eq!(variables.expand("$1"), "admin");
    }

    #[test]
    fn if_truthy_and_equality() {
        let mut variables = variables("/");
        variables.set("is_mobile", "0".to_owned());
        variables.set("mobile_ua", String::new());
        let operations = vec![RewritePhaseOpConfig::If {
            condition: IfConditionConfig {
                variable: "is_mobile".to_owned(),
                operator: IfOperatorConfig::Equals,
                value: Some("0".to_owned()),
            },
            body: vec![set("branch", "pc")],
        }];
        run(&operations, &mut variables);
        assert_eq!(variables.lookup("branch").as_deref(), Some("pc"));

        let operations = vec![RewritePhaseOpConfig::If {
            condition: IfConditionConfig {
                variable: "mobile_ua".to_owned(),
                operator: IfOperatorConfig::NegatedTruthy,
                value: None,
            },
            body: vec![set("fallback", "1")],
        }];
        run(&operations, &mut variables);
        assert_eq!(variables.lookup("fallback").as_deref(), Some("1"));
    }

    #[test]
    fn return_answers_immediately_and_stops_execution() {
        let mut variables = variables("/");
        let (control, _) = run(
            &[
                RewritePhaseOpConfig::Return {
                    status: 204,
                    body: None,
                    location: None,
                },
                set("never", "1"),
            ],
            &mut variables,
        );
        match control {
            PhaseControl::Respond(response) => {
                assert_eq!(response.status(), StatusCode::NO_CONTENT);
            }
            other => panic!("expected response, got {other:?}"),
        }
        assert_eq!(variables.lookup("never"), None);
    }

    #[test]
    fn rewrite_last_reports_reselection_with_captures() {
        let mut variables = variables("/old/path");
        let (control, _) = run(
            &[RewritePhaseOpConfig::Rewrite {
                pattern: "^/old/(.*)$".to_owned(),
                replacement: "/new/$1".to_owned(),
                flag: sdkwork_webserver_core::RewriteFlag::Last,
            }],
            &mut variables,
        );
        match control {
            PhaseControl::Rewrite {
                path,
                reselect,
                query,
            } => {
                assert_eq!(path, "/new/path");
                assert!(reselect);
                assert_eq!(query, None);
            }
            other => panic!("expected rewrite, got {other:?}"),
        }
    }

    #[test]
    fn conditional_add_header_is_recorded() {
        let mut variables = variables("/");
        variables.set("flag", "on".to_owned());
        let operations = vec![RewritePhaseOpConfig::If {
            condition: IfConditionConfig {
                variable: "flag".to_owned(),
                operator: IfOperatorConfig::Truthy,
                value: None,
            },
            body: vec![RewritePhaseOpConfig::AddHeader {
                name: "X-From".to_owned(),
                value: "cache-$host".to_owned(),
                always: false,
            }],
        }];
        let (_, headers) = run(&operations, &mut variables);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].value, "cache-api.example.com");
    }
}
