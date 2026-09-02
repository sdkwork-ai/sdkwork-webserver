use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::multipart::Form;
use reqwest::{Client, Method, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

pub type QueryParams = HashMap<String, Value>;
pub type RequestHeaders = HashMap<String, String>;

const SDKWORK_V3_ENVELOPE: bool = true;
const DEFAULT_MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Diagnostic budget for non-success streaming responses: small enough to
/// keep error handling bounded while still carrying a useful message.
const ERROR_RESPONSE_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct SdkworkConfig {
    pub base_url: String,
    pub timeout_ms: u64,
    pub max_response_body_bytes: usize,
    pub headers: RequestHeaders,
}

impl SdkworkConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_ms: 30_000,
            max_response_body_bytes: DEFAULT_MAX_RESPONSE_BODY_BYTES,
            headers: RequestHeaders::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SdkworkError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid header name: {0}")]
    InvalidHeaderName(#[from] reqwest::header::InvalidHeaderName),
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error("invalid http method: {0}")]
    InvalidHttpMethod(#[from] http::method::InvalidMethod),
    #[error("http status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("response body exceeds {maximum_bytes} bytes")]
    ResponseBodyTooLarge { maximum_bytes: usize },
    #[error("SDKWork API returned code {code} (traceId={trace_id})")]
    ApiStatus { code: i64, trace_id: String },
    #[error("access-token-only request requires Access-Token before request dispatch")]
    MissingAccessToken,
}

#[derive(Clone)]
pub struct SdkworkHttpClient {
    base_url: String,
    client: Client,
    headers: Arc<RwLock<RequestHeaders>>,
    max_response_body_bytes: usize,
}

pub struct SseStream<T> {
    events: VecDeque<Result<T, SdkworkError>>,
}

impl<T> SseStream<T> {
    pub fn next(&mut self) -> Option<Result<T, SdkworkError>> {
        self.events.pop_front()
    }
}

/// Bounded streaming reader over a binary response body. The 'next_chunk'
/// method yields one transport chunk at a time under the client's byte
/// budget, so a large payload is never materialized in memory
/// (PAGINATION_SPEC §2 / PERFORMANCE_SPEC bounded-body requirements).
pub struct BinaryResponseStream {
    response: Option<Response>,
    remaining_bytes: usize,
}

impl BinaryResponseStream {
    /// Bytes still available before the client's response budget is
    /// exhausted.
    pub fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }

    /// Declared 'Content-Length' of the response, when the server sent one.
    pub fn content_length(&self) -> Option<u64> {
        self.response.as_ref().and_then(|response| response.content_length())
    }

    /// Reads the next bounded chunk; 'Ok(None)' marks the end of the body.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        let Some(response) = self.response.as_mut() else {
            return Ok(None);
        };
        let Some(chunk) = response.chunk().await? else {
            self.response = None;
            return Ok(None);
        };
        if chunk.len() > self.remaining_bytes {
            self.response = None;
            return Err(SdkworkError::ResponseBodyTooLarge {
                maximum_bytes: self.remaining_bytes,
            });
        }
        self.remaining_bytes -= chunk.len();
        Ok(Some(chunk.to_vec()))
    }
}

impl SdkworkHttpClient {
    pub fn new(config: SdkworkConfig) -> Result<Self, SdkworkError> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(1)))
            .build()?;

        Ok(Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            client,
            headers: Arc::new(RwLock::new(config.headers)),
            max_response_body_bytes: config.max_response_body_bytes.max(1),
        })
    }
    pub fn set_auth_token(&self, token: impl Into<String>) {
        let mut headers = self.headers.write().expect("sdk headers poisoned");
        headers.insert("Authorization".to_string(), format!("Bearer {}", token.into()));
    }
    pub fn set_access_token(&self, token: impl Into<String>) {
        let mut headers = self.headers.write().expect("sdk headers poisoned");
        headers.insert("Access-Token".to_string(), token.into());
    }

    pub fn set_header(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut headers = self.headers.write().expect("sdk headers poisoned");
        headers.insert(key.into(), value.into());
    }

    pub async fn get<T>(
        &self,
        path: &str,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
    {
        self.request(Method::GET, path, query, Option::<&Value>::None, headers, None, false, false).await
    }

    pub async fn post<T, B>(
        &self,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request(Method::POST, path, query, body, headers, content_type, false, false).await
    }

    pub async fn put<T, B>(
        &self,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request(Method::PUT, path, query, body, headers, content_type, false, false).await
    }

    pub async fn patch<T, B>(
        &self,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request(Method::PATCH, path, query, body, headers, content_type, false, false).await
    }

    pub async fn delete<T>(
        &self,
        path: &str,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
    {
        self.request(Method::DELETE, path, query, Option::<&Value>::None, headers, None, false, false).await
    }

    pub async fn request_method<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.request(method, path, query, body, headers, content_type, skip_auth, access_token_only).await
    }

    pub async fn request_bytes<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<Vec<u8>, SdkworkError>
    where
        B: Serialize + ?Sized,
    {
        let mut request = self.client.request(method, self.build_url(path));
        if let Some(query_values) = query {
            request = request.query(&normalize_query(query_values));
        }
        request = request.headers(self.merge_headers(headers, skip_auth, access_token_only)?);
        if let Some(payload) = body {
            request = apply_body(request, payload, content_type)?;
        }
        let response = request.send().await?;
        decode_binary_response(response, self.max_response_body_bytes).await
    }

    /// Streams a binary response body in bounded chunks without
    /// materializing the whole payload in memory. Non-success statuses are
    /// read into a small diagnostic buffer before the error is returned.
    pub async fn request_bytes_stream<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<BinaryResponseStream, SdkworkError>
    where
        B: Serialize + ?Sized,
    {
        let mut request = self.client.request(method, self.build_url(path));
        if let Some(query_values) = query {
            request = request.query(&normalize_query(query_values));
        }
        request = request.headers(self.merge_headers(headers, skip_auth, access_token_only)?);
        if let Some(payload) = body {
            request = apply_body(request, payload, content_type)?;
        }
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            let error_body =
                read_response_body_bounded(response, ERROR_RESPONSE_BODY_BYTES).await?;
            return Err(SdkworkError::HttpStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&error_body).to_string(),
            });
        }
        Ok(BinaryResponseStream {
            response: Some(response),
            remaining_bytes: self.max_response_body_bytes,
        })
    }

    pub async fn stream<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&QueryParams>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<SseStream<T>, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let mut request = self.client.request(method, self.build_url(path));
        if let Some(query_values) = query {
            request = request.query(&normalize_query(query_values));
        }

        let mut merged_headers = self.merge_headers(headers, skip_auth, access_token_only)?;
        merged_headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        request = request.headers(merged_headers);

        if let Some(payload) = body {
            request = apply_body(request, payload, content_type)?;
        }

        let response = request.send().await?;
        let status = response.status();
        let body = read_response_body_bounded(response, self.max_response_body_bytes).await?;
        let body = String::from_utf8_lossy(&body).to_string();
        if !status.is_success() {
            return Err(SdkworkError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let mut events = VecDeque::new();
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') || !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim().to_string();
            if data == "[DONE]" {
                break;
            }
            events.push_back(serde_json::from_str::<T>(&data).map_err(SdkworkError::from));
        }

        Ok(SseStream { events })
    }

    async fn request<T, B>(
        &self,
        method: Method,
        path: &str,
        query: Option<&QueryParams>,
        body: Option<&B>,
        headers: Option<&RequestHeaders>,
        content_type: Option<&str>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<T, SdkworkError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let mut request = self.client.request(method, self.build_url(path));
        if let Some(query_values) = query {
            request = request.query(&normalize_query(query_values));
        }

        let merged_headers = self.merge_headers(headers, skip_auth, access_token_only)?;
        request = request.headers(merged_headers);

        if let Some(payload) = body {
            request = apply_body(request, payload, content_type)?;
        }

        let response = request.send().await?;
        decode_response(response, self.max_response_body_bytes).await
    }

    fn build_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        if path.starts_with('/') {
            return format!("{}{}", self.base_url, path);
        }
        format!("{}/{}", self.base_url, path)
    }

    fn merge_headers(
        &self,
        headers: Option<&RequestHeaders>,
        skip_auth: bool,
        access_token_only: bool,
    ) -> Result<HeaderMap, SdkworkError> {
        let mut merged = HeaderMap::new();
        let stored_headers = self.headers.read().expect("sdk headers poisoned");
        if !skip_auth && !access_token_only {
            for (key, value) in stored_headers.iter() {
                insert_header(&mut merged, key, value)?;
            }
        }
        if let Some(values) = headers {
            for (key, value) in values {
                if (!skip_auth && !access_token_only) || !is_credential_header(key) {
                    insert_header(&mut merged, key, value)?;
                }
            }
        }
        if access_token_only {
            let access_token = stored_headers.iter()
                .find(|(key, value)| key.eq_ignore_ascii_case("Access-Token") && !value.trim().is_empty())
                .map(|(_, value)| value.trim())
                .ok_or(SdkworkError::MissingAccessToken)?;
            insert_header(&mut merged, "Access-Token", access_token)?;
        }
        Ok(merged)
    }
}

fn is_credential_header(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization"
            | "access-token"
            | "x-api-key"
            | "x-tenant-id"
            | "x-organization-id"
            | "x-platform"
            | "x-user-id"
            | "x-sdkwork-tenant-id"
            | "x-sdkwork-organization-id"
            | "x-sdkwork-user-id"
    )
}

fn apply_body<B>(
    request: reqwest::RequestBuilder,
    body: &B,
    content_type: Option<&str>,
) -> Result<reqwest::RequestBuilder, SdkworkError>
where
    B: Serialize + ?Sized,
{
    let normalized_content_type = content_type.unwrap_or("application/json").trim().to_ascii_lowercase();
    if normalized_content_type.starts_with("multipart/form-data") {
        let payload = serde_json::to_value(body)?;
        return Ok(request.multipart(build_multipart_form(&payload)));
    }
    if normalized_content_type.starts_with("application/x-www-form-urlencoded") {
        return Ok(request.form(body));
    }

    let request = request.json(body);
    if !normalized_content_type.is_empty() && normalized_content_type != "application/json" {
        return Ok(request.header(CONTENT_TYPE, normalized_content_type));
    }
    Ok(request)
}

fn build_multipart_form(value: &Value) -> Form {
    match value {
        Value::Object(entries) => {
            let mut form = Form::new();
            for (key, field_value) in entries {
                form = append_form_value(form, key, field_value);
            }
            form
        }
        other => Form::new().text("value", stringify_value(other)),
    }
}

fn append_form_value(mut form: Form, key: &str, value: &Value) -> Form {
    match value {
        Value::Array(items) => {
            for item in items {
                form = append_form_value(form, key, item);
            }
            form
        }
        _ => form.text(key.to_string(), stringify_value(value)),
    }
}

fn normalize_query(query: &QueryParams) -> Vec<(String, String)> {
    query
        .iter()
        .map(|(key, value)| (key.clone(), stringify_value(value)))
        .collect()
}

fn stringify_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(inner) => inner.to_string(),
        Value::Number(inner) => inner.to_string(),
        Value::String(inner) => inner.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn insert_header(headers: &mut HeaderMap, key: &str, value: &str) -> Result<(), SdkworkError> {
    let name = HeaderName::from_bytes(key.as_bytes())?;
    let value = HeaderValue::from_str(value)?;
    headers.insert(name, value);
    Ok(())
}

async fn read_response_body_bounded(
    mut response: Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, SdkworkError> {
    if response.content_length().is_some_and(|length| length > maximum_bytes as u64) {
        return Err(SdkworkError::ResponseBodyTooLarge { maximum_bytes });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let next_length = body
            .len()
            .checked_add(chunk.len())
            .ok_or(SdkworkError::ResponseBodyTooLarge { maximum_bytes })?;
        if next_length > maximum_bytes {
            return Err(SdkworkError::ResponseBodyTooLarge { maximum_bytes });
        }
        body.try_reserve(chunk.len())
            .map_err(|_| SdkworkError::ResponseBodyTooLarge { maximum_bytes })?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn decode_response<T>(response: Response, maximum_bytes: usize) -> Result<T, SdkworkError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = read_response_body_bounded(response, maximum_bytes).await?;

    if !status.is_success() {
        return Err(SdkworkError::HttpStatus {
            status: status.as_u16(),
            body: String::from_utf8_lossy(&body).to_string(),
        });
    }

    if body.is_empty() {
        return Ok(serde_json::from_str("null")?);
    }

    if content_type.to_ascii_lowercase().contains("json") {
        let payload: Value = serde_json::from_slice(&body)?;
        if SDKWORK_V3_ENVELOPE {
            return decode_sdkwork_v3_payload(payload);
        }
        return Ok(serde_json::from_value(payload)?);
    }

    let text = String::from_utf8_lossy(&body).to_string();
    Ok(serde_json::from_value(Value::String(text))?)
}

async fn decode_binary_response(
    response: Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, SdkworkError> {
    let status = response.status();
    let body = read_response_body_bounded(response, maximum_bytes).await?;
    if !status.is_success() {
        return Err(SdkworkError::HttpStatus {
            status: status.as_u16(),
            body: String::from_utf8_lossy(&body).to_string(),
        });
    }
    Ok(body)
}

fn decode_sdkwork_v3_payload<T>(payload: Value) -> Result<T, SdkworkError>
where
    T: DeserializeOwned,
{
    let envelope = payload.as_object().ok_or_else(|| {
        SdkworkError::Serialization(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SDKWork response envelope must be an object",
        )))
    })?;
    let code = envelope.get("code").and_then(Value::as_i64).ok_or_else(|| {
        SdkworkError::Serialization(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SDKWork response envelope must contain an integer code",
        )))
    })?;
    let trace_id = envelope
        .get("traceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if code != 0 {
        return Err(SdkworkError::ApiStatus { code, trace_id });
    }
    let data = envelope.get("data").cloned().unwrap_or(Value::Null);
    let consumer_payload = data
        .as_object()
        .and_then(|object| object.get("item"))
        .cloned()
        .unwrap_or(data);
    Ok(serde_json::from_value(consumer_payload)?)
}
