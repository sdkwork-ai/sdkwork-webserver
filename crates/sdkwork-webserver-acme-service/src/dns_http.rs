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

/// Ceiling for the body excerpt rendered into an operator-facing error.
///
/// A wrong credential is answered by the provider with a diagnostic an operator
/// has to read, so the excerpt is generous enough to hold a sentence; it is
/// bounded because a misconfigured endpoint can answer with an error page far
/// larger than any JSON envelope.
pub(crate) const MAX_ERROR_BODY_EXCERPT_CHARS: usize = 512;

/// What a declared secret is replaced with before a provider body reaches an
/// error message.
pub(crate) const REDACTED: &str = "***";

/// JSON paths that carry an operator-actionable failure message, tried in
/// order.
///
/// Provider APIs are not uniform, so a fixed list covers the common envelope
/// shapes and the bounded raw excerpt stays the fallback rather than the
/// default. An operator with an unusual vendor names its own path through the
/// `HTTP_REQUEST` family's `errorPath`.
const ERROR_MESSAGE_PATHS: [&[&str]; 9] = [
    &["error", "message"],
    &["errors", "0", "message"],
    &["message"],
    &["Message"],
    &["msg"],
    &["detail"],
    &["error_description"],
    &["status", "message"],
    &["error"],
];

/// JSON paths carrying the vendor's own error code, which is what makes a
/// vendor's documentation and our message line up.
const ERROR_CODE_PATHS: [&[&str]; 5] = [
    &["error", "code"],
    &["errors", "0", "code"],
    &["code"],
    &["Code"],
    &["status", "code"],
];

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

    /// Fails with the provider's HTTP status and the diagnostic the provider
    /// itself returned.
    ///
    /// The body is the whole point: a rejected credential, an unowned zone and a
    /// missing permission are only distinguishable by what the provider says, so
    /// collapsing them into "HTTP 403" would leave an operator with nothing to
    /// act on.
    pub(crate) fn ensure_success(&self, provider: DnsProviderKind) -> AcmeServiceResult<()> {
        self.ensure_success_redacting(provider, &[])
    }

    /// [`Self::ensure_success`], with the caller's declared secrets removed from
    /// anything that reaches the message.
    ///
    /// A provider that echoes the request back into its error body would
    /// otherwise publish a token: the `HTTP_REQUEST` family carries its
    /// credential in an operator-authored template, and a template may
    /// legitimately place it in a body field. Redacting by value rather than by
    /// position is what makes that independent of where the operator put it.
    pub(crate) fn ensure_success_redacting(
        &self,
        provider: DnsProviderKind,
        secrets: &[String],
    ) -> AcmeServiceResult<()> {
        if self.is_success() {
            return Ok(());
        }
        let detail = self.provider_error_detail(secrets);
        Err(AcmeServiceError::provider(match detail {
            Some(detail) => format!(
                "{provider} rejected the request with HTTP {}: {detail}",
                self.status.as_u16()
            ),
            None => format!(
                "{provider} rejected the request with HTTP {} and returned no diagnostic",
                self.status.as_u16()
            ),
        }))
    }

    /// The diagnostic a failed provider call carries, as an operator can read
    /// it.
    ///
    /// A JSON error envelope is mined for its message (and code, when it has
    /// one) because that is the sentence the vendor wrote for a human; anything
    /// else falls back to a bounded excerpt of the raw body. `secrets` is
    /// whatever the caller declared as a credential, and it is removed from the
    /// result before it can reach a log or a console.
    pub(crate) fn provider_error_detail(&self, secrets: &[String]) -> Option<String> {
        if self.body.is_empty() {
            return None;
        }
        let detail = serde_json::from_slice::<serde_json::Value>(&self.body)
            .ok()
            .and_then(|value| error_message_from_json(&value))
            .unwrap_or_else(|| self.bounded_excerpt());
        let detail = redact_declared_secrets(&detail, secrets);
        (!detail.is_empty()).then_some(detail)
    }

    /// Up to [`MAX_ERROR_BODY_EXCERPT_CHARS`] characters of the body.
    ///
    /// Decoded as UTF-8 and then walked character by character. A previous
    /// implementation mapped each *byte* through `char::from(u8)` and kept only
    /// ASCII, which turned every multi-byte character into a space — every
    /// Chinese vendor's error message arrived as a row of blanks, which is worse
    /// than no message at all because it looks like the provider said nothing.
    /// Only control characters are collapsed, because they would forge log lines.
    fn bounded_excerpt(&self) -> String {
        let text = String::from_utf8_lossy(&self.body);
        let mut excerpt = String::new();
        for (index, character) in text.chars().enumerate() {
            if index >= MAX_ERROR_BODY_EXCERPT_CHARS {
                break;
            }
            excerpt.push(if character.is_control() {
                ' '
            } else {
                character
            });
        }
        excerpt.trim().to_string()
    }
}

/// Replaces every occurrence of a declared secret with [`REDACTED`].
///
/// Shared with the `HTTP_REQUEST` family, which resolves diagnostics from
/// operator-declared paths of its own and must redact them identically.
///
/// Short values are skipped: a one- or two-character "secret" would match inside
/// ordinary prose and shred the message it was meant to protect.
pub(crate) fn redact_declared_secrets(text: &str, secrets: &[String]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if secret.trim().chars().count() < 4 {
            continue;
        }
        if redacted.contains(secret.as_str()) {
            redacted = redacted.replace(secret.as_str(), REDACTED);
        }
    }
    redacted
}

/// Whether a rendered refusal carries one of `codes` as its vendor code.
///
/// [`error_message_from_json`] renders a vendor envelope as `{message} ({code})`,
/// so matching that parenthesised form is matching the *parsed* envelope rather
/// than guessing at prose. A code this function does not recognise simply does
/// not match, which is the safe direction: an unclassified refusal stays a
/// provider fault and is never mistaken for a configuration mistake.
///
/// The rendered form is pinned by [`tests::a_refusal_renders_its_vendor_code_where_this_matcher_expects_it`],
/// so a change to the format breaks the test instead of silently disarming every
/// classification that depends on it.
pub(crate) fn refusal_carries_code(message: &str, codes: &[&str]) -> bool {
    codes
        .iter()
        .any(|code| message.contains(&format!("({code})")))
}

/// Whether a rendered refusal is the provider rejecting the credential itself.
///
/// `401` and `403` are the only two statuses that mean "this caller may not do
/// this at all" rather than "this request was wrong"; every provider family here
/// answers a rejected key or token with one of them. Matched on the rendered
/// form for the same reason as [`refusal_carries_code`].
pub(crate) fn refusal_is_auth_status(message: &str) -> bool {
    message.contains("HTTP 401") || message.contains("HTTP 403")
}

/// Reads a message and a code out of a provider's JSON error envelope.
fn error_message_from_json(value: &serde_json::Value) -> Option<String> {
    let message = ERROR_MESSAGE_PATHS
        .iter()
        .find_map(|path| string_at(value, path));
    let code = ERROR_CODE_PATHS
        .iter()
        .find_map(|path| scalar_at(value, path));
    match (message, code) {
        (Some(message), Some(code)) => Some(format!("{message} ({code})")),
        (Some(message), None) => Some(message),
        // A bare code is still more than an operator had before: it is the
        // string the vendor's own documentation is indexed by.
        (None, Some(code)) => Some(format!("provider error code {code}")),
        (None, None) => None,
    }
}

/// Resolves a dot path (`result.id`, `errors.0.message`) inside a JSON value.
pub(crate) fn json_path_resolve<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path {
        current = match current {
            serde_json::Value::Object(map) => map.get(*segment)?,
            serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// A non-blank string at a path.
fn string_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    match json_path_resolve(value, path)? {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        _ => None,
    }
}

/// A non-blank string or a number at a path.
fn scalar_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    match json_path_resolve(value, path)? {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
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

    /// Client that also permits plaintext `http://` endpoints, for callers
    /// outside this crate.
    ///
    /// Exists for the domain-verification probe, which must be able to mirror
    /// what a plain-HTTP crawler sees: a domain that only answers over `http`
    /// is a different diagnosis from one that does not resolve at all. The
    /// timeout and response-body cap apply exactly as to the TLS-only client.
    pub fn new_permitting_plaintext() -> AcmeServiceResult<Self> {
        Self::build(true)
    }

    /// GETs `url` and returns the bounded response as `(status, body text)`.
    ///
    /// A read helper for callers that speak plain HTTP GET to an arbitrary
    /// endpoint (domain-ownership probes); the provider adapters keep using
    /// [`Self::send`] with their own request builders. The response body cap
    /// and the request timeout apply unchanged.
    pub async fn get_text(&self, url: &str) -> AcmeServiceResult<(u16, String)> {
        let request = json_request(DnsProviderKind::HttpRequest, Method::GET, url, None)?;
        let response = self.send(request).await?;
        let status = response.status.as_u16();
        let body = String::from_utf8(response.body.to_vec()).map_err(|error| {
            AcmeServiceError::provider(format!(
                "response body of {} bytes is not UTF-8 text: {error}",
                response.body.len()
            ))
        })?;
        Ok((status, body))
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

/// Builds a request whose method, content type and body come from an operator's
/// template rather than from a Rust adapter.
///
/// The body travels as bytes because a template may carry anything the vendor
/// accepts — JSON, a form body, a bare token. Sharing this builder keeps the
/// `Accept` / `User-Agent` conventions identical to the dedicated adapters, so a
/// template-driven provider is indistinguishable from a coded one at the wire.
pub(crate) fn templated_request(
    provider: DnsProviderKind,
    method: Method,
    url: &str,
    content_type: Option<&str>,
    body: Option<&str>,
) -> AcmeServiceResult<Request<Full<Bytes>>> {
    let payload = body
        .map(|body| Bytes::copy_from_slice(body.as_bytes()))
        .unwrap_or_default();
    let mut builder = Request::builder()
        .method(method)
        .uri(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, USER_AGENT_VALUE);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
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

    /// The provider's own sentence is what tells a rejected credential apart
    /// from an unowned zone, so it has to survive extraction intact.
    #[tokio::test]
    async fn a_json_error_envelope_yields_the_vendors_message_and_code() {
        let response = DnsApiResponse {
            status: StatusCode::FORBIDDEN,
            body: Bytes::from_static(
                br#"{"success":false,"errors":[{"code":10000,"message":"Authentication error"}]}"#,
            ),
        };
        let text = response
            .ensure_success(DnsProviderKind::Cloudflare)
            .expect_err("must fail")
            .to_string();
        assert!(text.contains("Authentication error"), "{text}");
        assert!(text.contains("10000"), "{text}");
        assert!(!text.contains("success"), "raw envelope leaked: {text}");
    }

    /// Every Chinese vendor answers a bad credential in Chinese. Mapping bytes
    /// through `char::from(u8)` erased exactly these messages, so this pins the
    /// shape that made the old implementation look like the provider said nothing.
    #[test]
    fn a_chinese_provider_message_survives_intact() {
        let response = DnsApiResponse {
            status: StatusCode::FORBIDDEN,
            body: Bytes::from(
                serde_json::to_vec(&serde_json::json!({
                    "code": "Forbidden",
                    "message": "该域名不属于当前账号",
                }))
                .expect("encode"),
            ),
        };
        let text = response
            .ensure_success(DnsProviderKind::AliyunDns)
            .expect_err("must fail")
            .to_string();
        assert!(text.contains("该域名不属于当前账号"), "{text}");
        assert!(text.contains("Forbidden"), "{text}");
    }

    /// The two classification helpers read a refusal as *text*, so the text they
    /// read has to be pinned.
    ///
    /// Every probe classification in the adapters is a predicate over this
    /// rendering: a change to the wording would silently disarm all of them —
    /// `refusal_carries_code` would stop matching and a wrong AccessKey would go
    /// back to looking like a provider hiccup, which is the failure this whole
    /// path exists to prevent. Asserting the rendering here means the format
    /// cannot move without this test going red first.
    #[test]
    fn a_refusal_renders_its_vendor_code_where_this_matcher_expects_it() {
        let envelope = DnsApiResponse {
            status: StatusCode::BAD_REQUEST,
            body: Bytes::from_static(
                br#"{"Code":"InvalidAccessKeyId.NotFound","Message":"The AccessKey does not exist."}"#,
            ),
        };
        let text = envelope
            .ensure_success(DnsProviderKind::AliyunDns)
            .expect_err("must fail")
            .to_string();
        assert!(
            refusal_carries_code(&text, &["InvalidAccessKeyId.NotFound"]),
            "the code is no longer where the matcher looks for it: {text}"
        );
        assert!(
            !refusal_carries_code(&text, &["SignatureDoesNotMatch"]),
            "the matcher must not match a code the vendor did not send: {text}"
        );
        assert!(
            !refusal_is_auth_status(&text),
            "HTTP 400 is not an authority refusal: {text}"
        );

        let refused = DnsApiResponse {
            status: StatusCode::UNAUTHORIZED,
            body: Bytes::from_static(
                br#"{"success":false,"errors":[{"code":10000,"message":"Authentication error"}]}"#,
            ),
        };
        let text = refused
            .ensure_success(DnsProviderKind::Cloudflare)
            .expect_err("must fail")
            .to_string();
        assert!(
            refusal_is_auth_status(&text),
            "the status is no longer where the matcher looks for it: {text}"
        );
        assert!(
            refusal_carries_code(&text, &["10000"]),
            "the Cloudflare authentication code is no longer rendered as a code: {text}"
        );
    }

    /// A refusal the matcher does not recognise must classify as *nothing*.
    ///
    /// The default direction matters more than any single code: an unread
    /// refusal stays a provider fault, so a family whose codes nobody has taught
    /// the matcher can never block an order on a message it does not understand.
    #[test]
    fn an_unrecognised_refusal_classifies_as_neither() {
        let response = DnsApiResponse {
            status: StatusCode::BAD_REQUEST,
            body: Bytes::from_static(br#"{"code":"SomeNewCode","message":"who knows"}"#),
        };
        let text = response
            .ensure_success(DnsProviderKind::Dnspod)
            .expect_err("must fail")
            .to_string();
        assert!(!refusal_carries_code(&text, &["-1"]), "{text}");
        assert!(!refusal_is_auth_status(&text), "{text}");
    }

    /// A non-JSON body (a WAF page, a plain-text refusal) still has to reach the
    /// operator as text rather than as blanks.
    #[test]
    fn a_non_json_body_keeps_its_multibyte_text_in_the_excerpt() {
        let response = DnsApiResponse {
            status: StatusCode::BAD_GATEWAY,
            body: Bytes::from("签名验证失败：请检查 AccessKeySecret".as_bytes().to_vec()),
        };
        let text = response
            .ensure_success(DnsProviderKind::AliyunDns)
            .expect_err("must fail")
            .to_string();
        assert!(text.contains("签名验证失败"), "{text}");
    }

    /// A vendor that echoes the request back must not echo the credential with
    /// it: the `HTTP_REQUEST` family carries its token in an operator-authored
    /// template, which may place it in a body field.
    #[test]
    fn declared_secrets_are_redacted_from_a_provider_body() {
        let token = "cf-super-secret-token-value";
        let response = DnsApiResponse {
            status: StatusCode::UNAUTHORIZED,
            body: Bytes::from(format!("token {token} is not valid").into_bytes()),
        };
        let text = response
            .ensure_success_redacting(DnsProviderKind::HttpRequest, &[token.to_string()])
            .expect_err("must fail")
            .to_string();
        assert!(!text.contains(token), "the secret was echoed: {text}");
        assert!(text.contains(REDACTED), "{text}");
        assert!(
            text.contains("is not valid"),
            "the diagnostic was lost: {text}"
        );
    }

    /// Redaction is by value and skips short entries: a two-character "secret"
    /// occurs inside ordinary prose and would shred the message it protects.
    #[test]
    fn a_short_declared_secret_is_left_alone() {
        let response = DnsApiResponse {
            status: StatusCode::BAD_REQUEST,
            body: Bytes::from_static(b"zone ab is not delegated"),
        };
        let text = response
            .ensure_success_redacting(DnsProviderKind::HttpRequest, &["ab".to_string()])
            .expect_err("must fail")
            .to_string();
        assert!(text.contains("zone ab is not delegated"), "{text}");
    }

    /// No diagnostic is not the same as a silent failure, so the message says so.
    #[test]
    fn an_empty_error_body_is_reported_rather_than_hidden() {
        let response = DnsApiResponse {
            status: StatusCode::FORBIDDEN,
            body: Bytes::new(),
        };
        let text = response
            .ensure_success(DnsProviderKind::Cloudflare)
            .expect_err("must fail")
            .to_string();
        assert!(text.contains("HTTP 403"), "{text}");
        assert!(text.contains("no diagnostic"), "{text}");
    }

    #[test]
    fn a_message_key_below_the_top_level_can_still_be_resolved() {
        assert_eq!(
            error_message_from_json(&serde_json::json!({ "error": { "message": "denied" } }))
                .as_deref(),
            Some("denied")
        );
        assert_eq!(
            error_message_from_json(&serde_json::json!({ "code": 40301 })).as_deref(),
            Some("provider error code 40301")
        );
        assert_eq!(
            error_message_from_json(&serde_json::json!({ "unrelated": true })),
            None
        );
    }
}
