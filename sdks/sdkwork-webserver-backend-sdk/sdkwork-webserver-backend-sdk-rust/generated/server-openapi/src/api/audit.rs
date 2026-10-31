use std::sync::Arc;

use crate::api::paths::append_query_string;
use crate::api::paths::backend_path;
use crate::http::{SdkworkError, SdkworkHttpClient};

#[derive(Clone)]
pub struct AuditApi {
    client: Arc<SdkworkHttpClient>,
}

impl AuditApi {
    pub fn new(client: Arc<SdkworkHttpClient>) -> Self {
        Self { client }
    }

    /// List audit logs
    pub async fn logs_list(
        &self,
        page_size: Option<i64>,
        cursor: Option<&str>,
        target_type: Option<&str>,
        action: Option<&str>,
        operator_id: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
            QueryParameterSpec::new("target_type", target_type, "form", true, false, None),
            QueryParameterSpec::new("action", action, "form", true, false, None),
            QueryParameterSpec::new("operator_id", operator_id, "form", true, false, None),
            QueryParameterSpec::new("start_date", start_date, "form", true, false, None),
            QueryParameterSpec::new("end_date", end_date, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/audit_logs".to_string()), &query);
        self.client.get(&path, None, None).await
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
