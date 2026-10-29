use std::sync::Arc;

use crate::api::base::RequestHeaders;
use crate::api::paths::append_query_string;
use crate::api::paths::backend_path;
use crate::http::{SdkworkError, SdkworkHttpClient};
use crate::models::{CreateServerRequest, CreateServerResponse};

#[derive(Clone)]
pub struct ServerApi {
    client: Arc<SdkworkHttpClient>,
}

impl ServerApi {
    pub fn new(client: Arc<SdkworkHttpClient>) -> Self {
        Self { client }
    }

    /// List managed servers
    pub async fn servers_list(
        &self,
        page_size: Option<i64>,
        cursor: Option<&str>,
    ) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/servers".to_string()), &query);
        self.client.get(&path, None, None).await
    }

    /// Register a managed server
    pub async fn servers_create(
        &self,
        body: &CreateServerRequest,
        idempotency_key: &str,
    ) -> Result<CreateServerResponse, SdkworkError> {
        let path = backend_path(&"/servers".to_string());
        let headers = build_request_headers(
            &[(
                "Idempotency-Key",
                HeaderParameterSpec::new(idempotency_key, "simple", false, None),
            )],
            &[],
        );
        self.client
            .post(
                &path,
                Some(body),
                None,
                headers.as_ref(),
                Some("application/json"),
            )
            .await
    }
}

struct HeaderParameterSpec {
    value: serde_json::Value,
    explode: bool,
    content_type: Option<&'static str>,
}

impl HeaderParameterSpec {
    fn new<T: serde::Serialize>(
        value: T,
        _style: &'static str,
        explode: bool,
        content_type: Option<&'static str>,
    ) -> Self {
        Self {
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
            explode,
            content_type,
        }
    }
}

fn build_request_headers(
    headers: &[(&str, HeaderParameterSpec)],
    cookies: &[(&str, HeaderParameterSpec)],
) -> Option<RequestHeaders> {
    let mut request_headers = RequestHeaders::new();
    for (name, parameter) in headers {
        if let Some(value) = serialize_header_parameter(parameter) {
            request_headers.insert((*name).to_string(), value);
        }
    }

    let cookie_header = build_cookie_header(cookies);
    if !cookie_header.is_empty() {
        request_headers
            .entry("Cookie".to_string())
            .and_modify(|existing| {
                existing.push_str("; ");
                existing.push_str(&cookie_header);
            })
            .or_insert(cookie_header);
    }

    if request_headers.is_empty() {
        None
    } else {
        Some(request_headers)
    }
}

fn build_cookie_header(cookies: &[(&str, HeaderParameterSpec)]) -> String {
    cookies
        .iter()
        .filter_map(|(name, value)| {
            serialize_header_parameter(value)
                .map(|value| format!("{}={}", percent_encode(name), percent_encode(&value)))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn serialize_header_parameter(parameter: &HeaderParameterSpec) -> Option<String> {
    if parameter.value.is_null() {
        return None;
    }
    if parameter.content_type.is_some() {
        return Some(parameter.value.to_string());
    }
    match &parameter.value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Array(values) => {
            let serialized = values
                .iter()
                .filter_map(serialize_json_value)
                .collect::<Vec<_>>();
            if serialized.is_empty() {
                None
            } else {
                Some(serialized.join(","))
            }
        }
        serde_json::Value::Object(values) => {
            let serialized = values
                .iter()
                .filter_map(|(key, value)| {
                    serialize_json_value(value).map(|serialized| {
                        if parameter.explode {
                            format!("{}={}", key, serialized)
                        } else {
                            format!("{},{}", key, serialized)
                        }
                    })
                })
                .collect::<Vec<_>>();
            if serialized.is_empty() {
                None
            } else {
                Some(serialized.join(","))
            }
        }
    }
}

fn serialize_json_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}

struct QueryParameterSpec<'a> {
    name: &'a str,
    value: serde_json::Value,
    style: &'a str,
    explode: bool,
    allow_reserved: bool,
    content_type: Option<&'a str>,
}

impl<'a> QueryParameterSpec<'a> {
    fn new<T: serde::Serialize>(
        name: &'a str,
        value: T,
        style: &'a str,
        explode: bool,
        allow_reserved: bool,
        content_type: Option<&'a str>,
    ) -> Self {
        Self {
            name,
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
            style,
            explode,
            allow_reserved,
            content_type,
        }
    }
}

fn build_query_string(parameters: &[QueryParameterSpec<'_>]) -> String {
    let mut pairs = Vec::new();
    for parameter in parameters {
        append_serialized_parameter(&mut pairs, parameter);
    }
    pairs.join("&")
}

fn append_serialized_parameter(pairs: &mut Vec<String>, parameter: &QueryParameterSpec<'_>) {
    if parameter.value.is_null() {
        return;
    }
    if parameter.content_type.is_some() {
        pairs.push(format!(
            "{}={}",
            percent_encode(parameter.name),
            encode_query_value(&parameter.value.to_string(), parameter.allow_reserved)
        ));
        return;
    }

    let style = if parameter.style.is_empty() {
        "form"
    } else {
        parameter.style
    };
    match &parameter.value {
        serde_json::Value::Array(values) => append_array_parameter(
            pairs,
            parameter.name,
            values,
            style,
            parameter.explode,
            parameter.allow_reserved,
        ),
        serde_json::Value::Object(values) if style == "deepObject" => {
            append_deep_object_parameter(pairs, parameter.name, values, parameter.allow_reserved)
        }
        serde_json::Value::Object(values) => append_object_parameter(
            pairs,
            parameter.name,
            values,
            style,
            parameter.explode,
            parameter.allow_reserved,
        ),
        value => pairs.push(format!(
            "{}={}",
            percent_encode(parameter.name),
            encode_query_value(&primitive_to_string(value), parameter.allow_reserved)
        )),
    }
}

fn append_array_parameter(
    pairs: &mut Vec<String>,
    name: &str,
    values: &[serde_json::Value],
    style: &str,
    explode: bool,
    allow_reserved: bool,
) {
    let serialized = values
        .iter()
        .filter(|value| !value.is_null())
        .map(primitive_to_string)
        .collect::<Vec<_>>();
    if serialized.is_empty() {
        return;
    }
    if style == "form" && explode {
        for item in serialized {
            pairs.push(format!(
                "{}={}",
                percent_encode(name),
                encode_query_value(&item, allow_reserved)
            ));
        }
        return;
    }
    pairs.push(format!(
        "{}={}",
        percent_encode(name),
        encode_query_value(&serialized.join(","), allow_reserved)
    ));
}

fn append_object_parameter(
    pairs: &mut Vec<String>,
    name: &str,
    values: &serde_json::Map<String, serde_json::Value>,
    style: &str,
    explode: bool,
    allow_reserved: bool,
) {
    let mut serialized = Vec::new();
    for (key, value) in values {
        if value.is_null() {
            continue;
        }
        if style == "form" && explode {
            pairs.push(format!(
                "{}={}",
                percent_encode(key),
                encode_query_value(&primitive_to_string(value), allow_reserved)
            ));
        } else {
            serialized.push(key.clone());
            serialized.push(primitive_to_string(value));
        }
    }
    if !serialized.is_empty() {
        pairs.push(format!(
            "{}={}",
            percent_encode(name),
            encode_query_value(&serialized.join(","), allow_reserved)
        ));
    }
}

fn append_deep_object_parameter(
    pairs: &mut Vec<String>,
    name: &str,
    values: &serde_json::Map<String, serde_json::Value>,
    allow_reserved: bool,
) {
    for (key, value) in values {
        if !value.is_null() {
            pairs.push(format!(
                "{}={}",
                percent_encode(&format!("{}[{}]", name, key)),
                encode_query_value(&primitive_to_string(value), allow_reserved)
            ));
        }
    }
}

fn encode_query_value(value: &str, allow_reserved: bool) -> String {
    let mut encoded = percent_encode(value);
    if !allow_reserved {
        return encoded;
    }
    for (escaped, reserved) in [
        ("%3A", ":"),
        ("%2F", "/"),
        ("%3F", "?"),
        ("%23", "#"),
        ("%5B", "["),
        ("%5D", "]"),
        ("%40", "@"),
        ("%21", "!"),
        ("%24", "$"),
        ("%26", "&"),
        ("%27", "'"),
        ("%28", "("),
        ("%29", ")"),
        ("%2A", "*"),
        ("%2B", "+"),
        ("%2C", ","),
        ("%3B", ";"),
        ("%3D", "="),
    ] {
        encoded = encoded.replace(escaped, reserved);
    }
    encoded
}

fn primitive_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{:02X}", byte).chars().collect(),
        })
        .collect()
}
