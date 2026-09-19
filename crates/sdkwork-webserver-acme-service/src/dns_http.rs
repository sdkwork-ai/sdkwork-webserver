//! Bounded HTTP client used by the DNS provider adapters.
//!
//! Provider APIs are a different trust boundary from the ACME directory: they
//! are plain JSON REST endpoints rather than an ACME protocol surface, so they
//! get their own client with an explicit response-size ceiling and a per
//! request timeout. Nothing here retries: a failed presentation must surface
//! to the order state machine, which owns bounded retry and backoff.

use std::time::Duration;

use bytes::Bytes;
use http::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::de::DeserializeOwned;

use crate::dns::DnsProviderKind;
use crate::{AcmeServiceError, AcmeServiceResult};

/// DNS API responses are small JSON documents; 256 KiB is generous.
pub(crate) const MAX_DNS_RESPONSE_BODY_BYTES: usize = 256 * 1024;

pub(crate) const DEFAULT_DNS_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) const USER_AGENT_VALUE: &str = "sdkwork-webserver-acme-service/0.1";

type DnsHyperClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

/// A completed DNS API response with its body already bounded.
pub(crate) struct DnsApiResponse {
    pub(crate) status: StatusCode,
    pub(crate) body: Bytes,
}

impl DnsApiResponse {
    pub(crate) fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Parses the body as JSON without echoing it in an error message.
    ///
    /// DNS API error bodies routinely quote the token or the record value back,
    /// so only the byte length is reported when parsing fails.
    pub(crate) fn json<T: DeserializeOwned>(
        &self,
        provider: DnsProviderKind,
    ) -> AcmeServiceResult<T> {
        serde_json::from_slice(&self.body).map_err(|error| {
            AcmeServiceError::provider(format!(
                "{provider} returned a body of {} bytes that is not the expected JSON: {error}",
                self.body.len()
            ))
        })
    }

    /// Fails with the provider's HTTP status and a truncated, secret-free body.
    pub(crate) fn ensure_success(&self, provider: DnsProviderKind) -> AcmeServiceResult<()> {
        if self.is_success() {
            return Ok(());
        }
        Err(AcmeServiceError::provider(format!(
            "{provider} rejected the request with HTTP {}{}",
            self.status.as_u16(),
            self.summarized_body()
        )))
    }

    /// Up to 512 bytes of the body, with control characters collapsed.
    fn summarized_body(&self) -> String {
        if self.body.is_empty() {
            return String::new();
        }
        let excerpt: String = self
            .body
            .iter()
            .take(512)
            .map(|byte| {
                let character = char::from(*byte);
                if character.is_ascii_graphic() || character == ' ' {
                    character
                } else {
                    ' '
                }
            })
            .collect();
        format!(": {excerpt}")
    }
}

/// Bounded DNS provider HTTP client.
///
/// Construct once per provider presenter; the underlying hyper client pools and
/// reuses connections.
pub struct DnsApiClient {
    client: DnsHyperClient,
    timeout: Duration,
}

impl DnsApiClient {
    /// Client for the public provider endpoints (HTTPS only).
    pub fn new() -> AcmeServiceResult<Self> {
        Self::build(false)
    }

    /// Client that also permits plaintext `http://` endpoints.
    ///
    /// Only reachable from crate-internal tests, which point a provider at a
    /// local stub server instead of the vendor API.
    #[cfg(test)]
    pub(crate) fn new_allowing_plaintext() -> AcmeServiceResult<Self> {
        Self::build(true)
    }

    /// Builds the connector with an explicit rustls crypto provider.
    ///
    /// The provider must be named here rather than inferred. This crate is
    /// consumed by binaries whose dependency graph links both `aws-lc-rs` and
    /// `ring` (the ACME HTTPS client and the database drivers disagree), and
    /// rustls panics instead of guessing when `from_crate_features()` cannot
    /// resolve a single default provider. Naming it also avoids
    /// `CryptoProvider::install_default()`, which would mutate the process-wide
    /// provider and change TLS behaviour for every other client in the host
    /// binary.
    fn build(allow_plaintext: bool) -> AcmeServiceResult<Self> {
        let builder = HttpsConnectorBuilder::new()
            .with_provider_and_platform_verifier(rustls::crypto::aws_lc_rs::default_provider())
            .map_err(|error| {
                AcmeServiceError::provider(format!("initialize DNS provider TLS verifier: {error}"))
            })?;
        let builder = if allow_plaintext {
            builder.https_or_http()
        } else {
            builder.https_only()
        };
        let connector = builder.enable_http1().enable_http2().build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Ok(Self {
            client,
            timeout: DEFAULT_DNS_REQUEST_TIMEOUT,
        })
    }

    /// Sends a request and returns a bounded response.
    pub(crate) async fn send(
        &self,
        request: Request<Full<Bytes>>,
    ) -> AcmeServiceResult<DnsApiResponse> {
        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| {
                AcmeServiceError::provider(format!(
                    "DNS provider request timed out after {} ms",
                    self.timeout.as_millis()
                ))
            })?
            .map_err(|error| {
                AcmeServiceError::provider(format!("DNS provider request: {error}"))
            })?;
        bounded_response(response).await
    }
}

/// Builds a JSON request with the headers every provider adapter expects.
pub(crate) fn json_request(
    provider: DnsProviderKind,
    method: Method,
    url: &str,
    body: Option<serde_json::Value>,
) -> AcmeServiceResult<Request<Full<Bytes>>> {
    let payload = match body {
        Some(value) => Bytes::from(serde_json::to_vec(&value).map_err(|error| {
            AcmeServiceError::Internal(format!("encode {provider} request body: {error}"))
        })?),
        None => Bytes::new(),
    };
    let mut builder = Request::builder()
        .method(method)
        .uri(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, USER_AGENT_VALUE);
    if !payload.is_empty() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    builder
        .body(Full::new(payload))
        .map_err(|error| AcmeServiceError::Internal(format!("build {provider} request: {error}")))
}

/// Builds a `application/x-www-form-urlencoded` request.
///
/// DNSPod is a form API rather than a JSON API, and its parameters can carry
/// vendor-specific reserved characters, so the caller supplies already-encoded
/// pairs and this function only joins them.
pub(crate) fn form_request(
    provider: DnsProviderKind,
    method: Method,
    url: &str,
    fields: &[(&str, &str)],
) -> AcmeServiceResult<Request<Full<Bytes>>> {
    let encoded = fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                encode_form_component(key),
                encode_form_component(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    Request::builder()
        .method(method)
        .uri(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Full::new(Bytes::from(encoded)))
        .map_err(|error| AcmeServiceError::Internal(format!("build {provider} request: {error}")))
}

/// `application/x-www-form-urlencoded` component encoding.
pub(crate) fn encode_form_component(value: &str) -> String {
    const FORM: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(value, FORM).to_string()
}

async fn bounded_response(response: Response<Incoming>) -> AcmeServiceResult<DnsApiResponse> {
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|error| {
            AcmeServiceError::provider(format!("read DNS provider response: {error}"))
        })?
        .to_bytes();
    if body.len() > MAX_DNS_RESPONSE_BODY_BYTES {
        return Err(AcmeServiceError::provider(format!(
            "DNS provider response of {} bytes exceeds {MAX_DNS_RESPONSE_BODY_BYTES}",
            body.len()
        )));
    }
    Ok(DnsApiResponse { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_carries_json_headers_only_when_a_body_is_present() {
        let with_body = json_request(
            DnsProviderKind::Cloudflare,
            Method::POST,
            "https://api.cloudflare.com/client/v4/zones",
            Some(serde_json::json!({ "type": "TXT" })),
        )
        .expect("build");
        assert_eq!(
            with_body.headers().get(CONTENT_TYPE).expect("content-type"),
            "application/json"
        );

        let without_body = json_request(
            DnsProviderKind::Cloudflare,
            Method::GET,
            "https://api.cloudflare.com/client/v4/zones",
            None,
        )
        .expect("build");
        assert!(without_body.headers().get(CONTENT_TYPE).is_none());
        assert_eq!(
            without_body.headers().get(USER_AGENT).expect("user-agent"),
            USER_AGENT_VALUE
        );
    }

    #[test]
    fn error_summary_is_bounded_and_secret_free_of_control_bytes() {
        let response = DnsApiResponse {
            status: StatusCode::UNAUTHORIZED,
            body: Bytes::from(vec![b'a'; 4_096]),
        };
        let message = response
            .ensure_success(DnsProviderKind::Dnspod)
            .expect_err("must fail");
        let text = message.to_string();
        assert!(text.contains("HTTP 401"));
        // 512-byte excerpt plus the surrounding sentence, never the full body.
        assert!(text.len() < 1_024, "unbounded summary: {}", text.len());

        let control = DnsApiResponse {
            status: StatusCode::BAD_REQUEST,
            body: Bytes::from_static(b"line\nbreak\r\ttab"),
        };
        let text = control
            .ensure_success(DnsProviderKind::AliyunDns)
            .expect_err("must fail")
            .to_string();
        assert!(!text.contains('\n'));
        assert!(!text.contains('\t'));
    }

    #[test]
    fn success_status_short_circuits() {
        let response = DnsApiResponse {
            status: StatusCode::OK,
            body: Bytes::new(),
        };
        response
            .ensure_success(DnsProviderKind::Cloudflare)
            .expect("2xx is success");
    }

    #[test]
    fn json_parse_failure_reports_only_the_length() {
        let response = DnsApiResponse {
            status: StatusCode::OK,
            body: Bytes::from_static(b"<html>not json</html>"),
        };
        let message = response
            .json::<serde_json::Value>(DnsProviderKind::Cloudflare)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("21 bytes"));
        assert!(!message.contains("html"));
    }
}
