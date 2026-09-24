use std::sync::Arc;

use crate::api::base::{RequestHeaders};
use crate::api::paths::backend_path;
use crate::api::paths::append_query_string;
use crate::http::{SdkworkError, SdkworkHttpClient};
use crate::models::{ClusterHostResponse, ClusterInstanceResponse, ClusterOverviewResponse, ClusterProbeRunResponse, ClusterResponse, ClusterSyncManifest, CreateClusterRequest, EnqueueClusterPeerMessagesRequest, EnqueueClusterPeerMessagesResponse, ProbeClusterInstanceRequest, PublishClusterSyncRequest, UpdateClusterHostRequest, UpdateClusterInstanceRequest, UpdateClusterRequest};

#[derive(Clone)]
pub struct ClusterApi {
    client: Arc<SdkworkHttpClient>,
}

impl ClusterApi {
    pub fn new(client: Arc<SdkworkHttpClient>) -> Self {
        Self { client }
    }

    /// List Web Server clusters
    pub async fn clusters_list(&self, page: Option<i64>, page_size: Option<i64>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page", page, "form", true, false, None),
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/clusters".to_string()), &query);
        self.client.get(&path, None, None).await
    }

    /// Create a Web Server cluster
    pub async fn clusters_create(&self, body: &CreateClusterRequest, idempotency_key: &str) -> Result<ClusterResponse, SdkworkError> {
        let path = backend_path(&"/clusters".to_string());
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.post(&path, Some(body), None, headers.as_ref(), Some("application/json")).await
    }

    /// Retrieve a Web Server cluster
    pub async fn clusters_retrieve(&self, cluster_id: &str) -> Result<ClusterResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/{}", serialize_path_parameter(cluster_id, PathParameterSpec::new("clusterId", "simple", false))));
        self.client.get(&path, None, None).await
    }

    /// Update a Web Server cluster
    pub async fn clusters_update(&self, cluster_id: &str, body: &UpdateClusterRequest, idempotency_key: &str) -> Result<ClusterResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/{}", serialize_path_parameter(cluster_id, PathParameterSpec::new("clusterId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.patch(&path, Some(body), None, headers.as_ref(), Some("application/json")).await
    }

    /// Delete an empty Web Server cluster
    pub async fn clusters_delete(&self, cluster_id: &str, idempotency_key: &str) -> Result<(), SdkworkError> {
        let path = backend_path(&format!("/clusters/{}", serialize_path_parameter(cluster_id, PathParameterSpec::new("clusterId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.delete(&path, None, headers.as_ref()).await
    }

    /// Publish a desired-state revision to every instance of the cluster
    pub async fn clusters_sync(&self, cluster_id: &str, body: &PublishClusterSyncRequest) -> Result<ClusterSyncManifest, SdkworkError> {
        let path = backend_path(&format!("/clusters/{}/sync", serialize_path_parameter(cluster_id, PathParameterSpec::new("clusterId", "simple", false))));
        self.client.post(&path, Some(body), None, None, Some("application/json")).await
    }

    /// List cluster hosts with system and network identity
    pub async fn clusters_hosts_list(&self, page_size: Option<i64>, cursor: Option<&str>, cluster_id: Option<&str>, status: Option<i64>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
            QueryParameterSpec::new("cluster_id", cluster_id, "form", true, false, None),
            QueryParameterSpec::new("status", status, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/clusters/hosts".to_string()), &query);
        self.client.get(&path, None, None).await
    }

    /// Retrieve a cluster host
    pub async fn clusters_hosts_retrieve(&self, host_id: &str) -> Result<ClusterHostResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/hosts/{}", serialize_path_parameter(host_id, PathParameterSpec::new("hostId", "simple", false))));
        self.client.get(&path, None, None).await
    }

    /// Rename a host or reassign it to another cluster
    pub async fn clusters_hosts_update(&self, host_id: &str, body: &UpdateClusterHostRequest, idempotency_key: &str) -> Result<ClusterHostResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/hosts/{}", serialize_path_parameter(host_id, PathParameterSpec::new("hostId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.patch(&path, Some(body), None, headers.as_ref(), Some("application/json")).await
    }

    /// Remove an instance-free host from the cluster inventory
    pub async fn clusters_hosts_delete(&self, host_id: &str, idempotency_key: &str) -> Result<(), SdkworkError> {
        let path = backend_path(&format!("/clusters/hosts/{}", serialize_path_parameter(host_id, PathParameterSpec::new("hostId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.delete(&path, None, headers.as_ref()).await
    }

    /// List webserver process instances with liveness state
    pub async fn clusters_instances_list(&self, page_size: Option<i64>, cursor: Option<&str>, cluster_id: Option<&str>, host_id: Option<&str>, status: Option<i64>, health_state: Option<&str>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
            QueryParameterSpec::new("cluster_id", cluster_id, "form", true, false, None),
            QueryParameterSpec::new("host_id", host_id, "form", true, false, None),
            QueryParameterSpec::new("status", status, "form", true, false, None),
            QueryParameterSpec::new("health_state", health_state, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/clusters/instances".to_string()), &query);
        self.client.get(&path, None, None).await
    }

    /// Retrieve a webserver process instance
    pub async fn clusters_instances_retrieve(&self, instance_id: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.get(&path, None, None).await
    }

    /// Update an instance display name, status, or advertised endpoint
    pub async fn clusters_instances_update(&self, instance_id: &str, body: &UpdateClusterInstanceRequest, idempotency_key: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.patch(&path, Some(body), None, headers.as_ref(), Some("application/json")).await
    }

    /// Unregister a webserver process instance
    pub async fn clusters_instances_delete(&self, instance_id: &str, idempotency_key: &str) -> Result<(), SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.delete(&path, None, headers.as_ref()).await
    }

    /// List cluster lifecycle events
    pub async fn clusters_events_list(&self, page_size: Option<i64>, cursor: Option<&str>, cluster_id: Option<&str>, severity: Option<&str>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
            QueryParameterSpec::new("cluster_id", cluster_id, "form", true, false, None),
            QueryParameterSpec::new("severity", severity, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&"/clusters/events".to_string()), &query);
        self.client.get(&path, None, None).await
    }

    /// Retrieve the cluster health overview for status polling
    pub async fn clusters_overview_retrieve(&self) -> Result<ClusterOverviewResponse, SdkworkError> {
        let path = backend_path(&"/clusters/overview".to_string());
        self.client.get(&path, None, None).await
    }

    /// List one instance's stored heartbeat samples
    pub async fn clusters_instances_heartbeats_list(&self, instance_id: &str, page_size: Option<i64>, cursor: Option<&str>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&format!("/clusters/instances/{}/heartbeats", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false)))), &query);
        self.client.get(&path, None, None).await
    }

    /// List one instance's heartbeat metric samples for trend charts
    pub async fn clusters_instances_metrics_list(&self, instance_id: &str, page_size: Option<i64>, cursor: Option<&str>) -> Result<serde_json::Value, SdkworkError> {
        let query = build_query_string(&[
            QueryParameterSpec::new("page_size", page_size, "form", true, false, None),
            QueryParameterSpec::new("cursor", cursor, "form", true, false, None),
        ]);
        let path = append_query_string(backend_path(&format!("/clusters/instances/{}/metrics/history", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false)))), &query);
        self.client.get(&path, None, None).await
    }

    /// Probe one instance's connectivity and record the outcome
    pub async fn clusters_instances_probe(&self, instance_id: &str, body: &ProbeClusterInstanceRequest) -> Result<ClusterProbeRunResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}/probe", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.post(&path, Some(body), None, None, Some("application/json")).await
    }

    /// Gracefully drain one instance out of routing
    pub async fn clusters_instances_drain(&self, instance_id: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}/drain", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.post(&path, Option::<&serde_json::Value>::None, None, None, None).await
    }

    /// Clear the drain flag and restore routing participation
    pub async fn clusters_instances_undrain(&self, instance_id: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}/undrain", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.post(&path, Option::<&serde_json::Value>::None, None, None, None).await
    }

    /// Cordon one instance out of routing without draining it
    pub async fn clusters_instances_cordon(&self, instance_id: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}/cordon", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.post(&path, Option::<&serde_json::Value>::None, None, None, None).await
    }

    /// Uncordon one instance back into routing
    pub async fn clusters_instances_uncordon(&self, instance_id: &str) -> Result<ClusterInstanceResponse, SdkworkError> {
        let path = backend_path(&format!("/clusters/instances/{}/uncordon", serialize_path_parameter(instance_id, PathParameterSpec::new("instanceId", "simple", false))));
        self.client.post(&path, Option::<&serde_json::Value>::None, None, None, None).await
    }

    /// Enqueue a peer message to one instance or broadcast to online members
    pub async fn clusters_messages_create(&self, body: &EnqueueClusterPeerMessagesRequest, idempotency_key: &str) -> Result<EnqueueClusterPeerMessagesResponse, SdkworkError> {
        let path = backend_path(&"/clusters/messages".to_string());
        let headers = build_request_headers(
            &[
                ("Idempotency-Key", HeaderParameterSpec::new(idempotency_key, "simple", false, None)),
            ],
            &[],
        );
        self.client.post(&path, Some(body), None, headers.as_ref(), Some("application/json")).await
    }

}

struct PathParameterSpec<'a> {
    name: &'a str,
    style: &'a str,
    explode: bool,
}

impl<'a> PathParameterSpec<'a> {
    fn new(name: &'a str, style: &'a str, explode: bool) -> Self {
        Self { name, style, explode }
    }
}

fn serialize_path_parameter<T: serde::Serialize>(value: T, spec: PathParameterSpec<'_>) -> String {
    let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    if value.is_null() {
        return String::new();
    }
    let style = if spec.style.is_empty() { "simple" } else { spec.style };
    match value {
        serde_json::Value::Array(values) => serialize_path_array(spec.name, &values, style, spec.explode),
        serde_json::Value::Object(values) => serialize_path_object(spec.name, &values, style, spec.explode),
        value => format!("{}{}", path_primitive_prefix(spec.name, style), percent_encode(&primitive_to_string(&value))),
    }
}

fn serialize_path_array(name: &str, values: &[serde_json::Value], style: &str, explode: bool) -> String {
    let serialized = values
        .iter()
        .filter(|value| !value.is_null())
        .map(|value| percent_encode(&primitive_to_string(value)))
        .collect::<Vec<_>>();
    if serialized.is_empty() {
        return path_prefix(name, style);
    }
    if style == "matrix" {
        if explode {
            return serialized.iter().map(|item| format!(";{}={}", name, item)).collect::<Vec<_>>().join("");
        }
        return format!(";{}={}", name, serialized.join(","));
    }
    let separator = if explode { "." } else { "," };
    format!("{}{}", path_prefix(name, style), serialized.join(separator))
}

fn serialize_path_object(
    name: &str,
    values: &serde_json::Map<String, serde_json::Value>,
    style: &str,
    explode: bool,
) -> String {
    let mut entries = Vec::new();
    let mut exploded = Vec::new();
    for (key, value) in values {
        if value.is_null() {
            continue;
        }
        let escaped_key = percent_encode(key);
        let escaped_value = percent_encode(&primitive_to_string(value));
        if explode {
            if style == "matrix" {
                exploded.push(format!(";{}={}", escaped_key, escaped_value));
            } else {
                exploded.push(format!("{}={}", escaped_key, escaped_value));
            }
        } else {
            entries.push(escaped_key);
            entries.push(escaped_value);
        }
    }
    if style == "matrix" {
        if explode {
            return exploded.join("");
        }
        return format!(";{}={}", name, entries.join(","));
    }
    if explode {
        let separator = if style == "label" { "." } else { "," };
        return format!("{}{}", path_prefix(name, style), exploded.join(separator));
    }
    format!("{}{}", path_prefix(name, style), entries.join(","))
}

fn path_prefix(name: &str, style: &str) -> String {
    match style {
        "label" => ".".to_string(),
        "matrix" => format!(";{}", name),
        _ => String::new(),
    }
}

fn path_primitive_prefix(name: &str, style: &str) -> String {
    if style == "matrix" {
        format!(";{}=", name)
    } else {
        path_prefix(name, style)
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

fn build_request_headers(headers: &[(&str, HeaderParameterSpec)], cookies: &[(&str, HeaderParameterSpec)]) -> Option<RequestHeaders> {
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

    let style = if parameter.style.is_empty() { "form" } else { parameter.style };
    match &parameter.value {
        serde_json::Value::Array(values) => append_array_parameter(pairs, parameter.name, values, style, parameter.explode, parameter.allow_reserved),
        serde_json::Value::Object(values) if style == "deepObject" => append_deep_object_parameter(pairs, parameter.name, values, parameter.allow_reserved),
        serde_json::Value::Object(values) => append_object_parameter(pairs, parameter.name, values, style, parameter.explode, parameter.allow_reserved),
        value => pairs.push(format!("{}={}", percent_encode(parameter.name), encode_query_value(&primitive_to_string(value), parameter.allow_reserved))),
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
    let serialized = values.iter().filter(|value| !value.is_null()).map(primitive_to_string).collect::<Vec<_>>();
    if serialized.is_empty() {
        return;
    }
    if style == "form" && explode {
        for item in serialized {
            pairs.push(format!("{}={}", percent_encode(name), encode_query_value(&item, allow_reserved)));
        }
        return;
    }
    pairs.push(format!("{}={}", percent_encode(name), encode_query_value(&serialized.join(","), allow_reserved)));
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
            pairs.push(format!("{}={}", percent_encode(key), encode_query_value(&primitive_to_string(value), allow_reserved)));
        } else {
            serialized.push(key.clone());
            serialized.push(primitive_to_string(value));
        }
    }
    if !serialized.is_empty() {
        pairs.push(format!("{}={}", percent_encode(name), encode_query_value(&serialized.join(","), allow_reserved)));
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
            pairs.push(format!("{}={}", percent_encode(&format!("{}[{}]", name, key)), encode_query_value(&primitive_to_string(value), allow_reserved)));
        }
    }
}

fn encode_query_value(value: &str, allow_reserved: bool) -> String {
    let mut encoded = percent_encode(value);
    if !allow_reserved {
        return encoded;
    }
    for (escaped, reserved) in [
        ("%3A", ":"), ("%2F", "/"), ("%3F", "?"), ("%23", "#"),
        ("%5B", "["), ("%5D", "]"), ("%40", "@"), ("%21", "!"),
        ("%24", "$"), ("%26", "&"), ("%27", "'"), ("%28", "("),
        ("%29", ")"), ("%2A", "*"), ("%2B", "+"), ("%2C", ","),
        ("%3B", ";"), ("%3D", "="),
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
