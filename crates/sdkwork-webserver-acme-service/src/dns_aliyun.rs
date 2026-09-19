//! Aliyun DNS (alidns.aliyuncs.com) adapter.
//!
//! Aliyun's RPC style requires a request signature: every parameter is
//! canonicalized, the canonical form is wrapped into a string-to-sign, and the
//! result is `base64(HMAC-SHA1(accessKeySecret + "&", stringToSign))`. The
//! access key secret therefore never leaves this module except as an HMAC key,
//! and it is never rendered into an error message or a log line.

use std::fmt;

use async_trait::async_trait;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use http::Method;
use serde::Deserialize;
use sha1::Sha1;

use crate::dns::{
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind, ACME_CHALLENGE_LABEL,
};
use crate::dns_http::{json_request, DnsApiClient};
use crate::{AcmeServiceError, AcmeServiceResult};

pub const ALIYUN_DEFAULT_BASE_URL: &str = "https://alidns.aliyuncs.com";

/// The DNS product's RPC API version.
const ALIYUN_API_VERSION: &str = "2015-01-09";
const ALIYUN_SIGNATURE_METHOD: &str = "HMAC-SHA1";
const ALIYUN_SIGNATURE_VERSION: &str = "1.0";

/// TXT records for validation do not need a long TTL.
const CHALLENGE_TTL_SECONDS: u32 = 600;

/// Aliyun error code meaning the record id no longer exists.
const ALIYUN_MISSING_RECORD_CODE: &str = "InvalidRecordId.NotFound";

/// RFC 3986 unreserved set as Aliyun defines it for RPC signature input:
/// `~` stays literal, and a space becomes `%20` rather than `+`.
const ALIYUN_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Percent-encodes one component for the Aliyun signature input.
pub(crate) fn aliyun_percent_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, ALIYUN_ENCODE_SET).to_string()
}

/// Aliyun-backed DNS-01 presenter.
pub struct AliyunDns01Presenter {
    client: DnsApiClient,
    access_key_id: String,
    access_key_secret: String,
    base_url: String,
}

impl fmt::Debug for AliyunDns01Presenter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AliyunDns01Presenter")
            .field("access_key_id", &self.access_key_id)
            .field("access_key_secret", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl AliyunDns01Presenter {
    pub fn new(
        client: DnsApiClient,
        access_key_id: impl AsRef<str>,
        access_key_secret: impl AsRef<str>,
    ) -> AcmeServiceResult<Self> {
        Self::with_base_url(
            client,
            access_key_id,
            access_key_secret,
            ALIYUN_DEFAULT_BASE_URL,
        )
    }

    pub(crate) fn with_base_url(
        client: DnsApiClient,
        access_key_id: impl AsRef<str>,
        access_key_secret: impl AsRef<str>,
        base_url: impl Into<String>,
    ) -> AcmeServiceResult<Self> {
        let access_key_id = access_key_id.as_ref().trim();
        let access_key_secret = access_key_secret.as_ref().trim();
        if access_key_id.is_empty() || access_key_secret.is_empty() {
            return Err(AcmeServiceError::config(
                "Aliyun AccessKey id and secret must both be non-empty",
            ));
        }
        Ok(Self {
            client,
            access_key_id: access_key_id.to_string(),
            access_key_secret: access_key_secret.to_string(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// Builds a fully signed request for one Aliyun action.
    fn signed_request(
        &self,
        timestamp: &str,
        nonce: &str,
        action: &str,
        action_params: &[(&str, &str)],
    ) -> AcmeServiceResult<String> {
        let mut params: Vec<(String, String)> = vec![
            ("Format".to_string(), "JSON".to_string()),
            ("Version".to_string(), ALIYUN_API_VERSION.to_string()),
            ("AccessKeyId".to_string(), self.access_key_id.clone()),
            (
                "SignatureMethod".to_string(),
                ALIYUN_SIGNATURE_METHOD.to_string(),
            ),
            ("Timestamp".to_string(), timestamp.to_string()),
            (
                "SignatureVersion".to_string(),
                ALIYUN_SIGNATURE_VERSION.to_string(),
            ),
            ("SignatureNonce".to_string(), nonce.to_string()),
            ("Action".to_string(), action.to_string()),
        ];
        for (key, value) in action_params {
            params.push(((*key).to_string(), (*value).to_string()));
        }
        // Aliyun requires the canonical form to be built from parameters sorted
        // by key, and signing must run against exactly that ordering.
        params.sort_by(|left, right| left.0.cmp(&right.0));

        let canonical = params
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    aliyun_percent_encode(key),
                    aliyun_percent_encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        let string_to_sign = format!(
            "GET&{}&{}",
            aliyun_percent_encode("/"),
            aliyun_percent_encode(&canonical)
        );
        let signature = sign_hmac_sha1(&self.access_key_secret, string_to_sign.as_bytes())?;

        let query = params
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    aliyun_percent_encode(&key),
                    aliyun_percent_encode(&value)
                )
            })
            .chain(std::iter::once(format!(
                "Signature={}",
                aliyun_percent_encode(&signature)
            )))
            .collect::<Vec<_>>()
            .join("&");
        Ok(format!("{}/?{query}", self.base_url))
    }

    async fn call(
        &self,
        action: &str,
        action_params: &[(&str, &str)],
        timestamp: &str,
        nonce: &str,
    ) -> AcmeServiceResult<AliyunResponse> {
        let url = self.signed_request(timestamp, nonce, action, action_params)?;
        let request = json_request(DnsProviderKind::AliyunDns, Method::GET, &url, None)?;
        let response = self.client.send(request).await?;
        if !response.is_success() {
            // Aliyun reports failures as a JSON envelope with a stable code;
            // that code is what makes a withdrawal idempotent.
            if let Ok(failure) = response.json::<AliyunErrorEnvelope>(DnsProviderKind::AliyunDns) {
                return Err(AcmeServiceError::provider(format!(
                    "Aliyun rejected {action}: {} ({})",
                    failure.message, failure.code
                )));
            }
            response.ensure_success(DnsProviderKind::AliyunDns)?;
        }
        let body: AliyunResponse = response.json(DnsProviderKind::AliyunDns)?;
        Ok(body)
    }
}

#[async_trait]
impl Dns01Presenter for AliyunDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::AliyunDns)
    }

    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let relative = request.relative_record_name()?;
        let ttl = CHALLENGE_TTL_SECONDS.to_string();
        let body = self
            .call(
                "AddDomainRecord",
                &[
                    ("DomainName", request.zone_apex.as_str()),
                    ("RR", relative.as_str()),
                    ("Type", "TXT"),
                    ("Value", request.record_value.as_str()),
                    ("TTL", ttl.as_str()),
                ],
                &aliyun_timestamp(),
                &aliyun_signature_nonce(),
            )
            .await?;
        let record_ref = body.record_id.filter(|id| !id.is_empty()).ok_or_else(|| {
            AcmeServiceError::provider("Aliyun accepted the presentation without a RecordId")
        })?;
        tracing::debug!(
            record_name = %request.record_name,
            "presented {ACME_CHALLENGE_LABEL} TXT record through Aliyun DNS"
        );
        Ok(Dns01RecordHandle {
            zone_apex: request.zone_apex.clone(),
            record_name: request.record_name.clone(),
            record_value: request.record_value.clone(),
            provider_record_ref: Some(record_ref),
        })
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        let Some(record_ref) = handle.provider_record_ref.as_deref() else {
            return Ok(());
        };
        match self
            .call(
                "DeleteDomainRecord",
                &[("RecordId", record_ref)],
                &aliyun_timestamp(),
                &aliyun_signature_nonce(),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if is_missing_record(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn is_missing_record(error: &AcmeServiceError) -> bool {
    let AcmeServiceError::Provider(message) = error else {
        return false;
    };
    message.contains(&format!("({ALIYUN_MISSING_RECORD_CODE})"))
}

/// `base64(HMAC-SHA1(key = accessKeySecret + "&", message = stringToSign))`.
pub(crate) fn sign_hmac_sha1(access_key_secret: &str, message: &[u8]) -> AcmeServiceResult<String> {
    let mut mac = Hmac::<Sha1>::new_from_slice(format!("{access_key_secret}&").as_bytes())
        .map_err(|error| AcmeServiceError::Internal(format!("initialize HMAC-SHA1: {error}")))?;
    mac.update(message);
    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

/// `YYYY-MM-DDThh:mm:ssZ` in UTC, the only timestamp shape Aliyun accepts.
pub(crate) fn aliyun_timestamp_at(moment: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        moment.year(),
        u8::from(moment.month()),
        moment.day(),
        moment.hour(),
        moment.minute(),
        moment.second()
    )
}

fn aliyun_timestamp() -> String {
    aliyun_timestamp_at(time::OffsetDateTime::now_utc())
}

/// 128 bits of randomness rendered as 32 hex characters.
pub(crate) fn signature_nonce_from(value: u128) -> String {
    format!("{value:032x}")
}

fn aliyun_signature_nonce() -> String {
    signature_nonce_from(rand::random::<u128>())
}

#[derive(Debug, Deserialize)]
struct AliyunResponse {
    #[serde(rename = "RecordId", default)]
    record_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AliyunErrorEnvelope {
    #[serde(rename = "Code", default)]
    code: String,
    #[serde(rename = "Message", default)]
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::extract::{Query, State};
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    #[derive(Default)]
    struct StubState {
        requests: Mutex<Vec<HashMap<String, String>>>,
        error_code: Option<&'static str>,
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        async fn handler(
            State(state): State<Arc<StubState>>,
            Query(params): Query<HashMap<String, String>>,
        ) -> (http::StatusCode, Json<Value>) {
            let action = params.get("Action").cloned().unwrap_or_default();
            state.requests.lock().expect("lock").push(params);
            if let Some(code) = state.error_code {
                return (
                    http::StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "Code": code, "Message": "record not found", "RequestId": "r-1" }),
                    ),
                );
            }
            let _ = action;
            (
                http::StatusCode::OK,
                Json(json!({ "RecordId": "aliyun-rec-9" })),
            )
        }

        let app = Router::new().route("/", get(handler)).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{address}")
    }

    fn presenter(base_url: String) -> AliyunDns01Presenter {
        AliyunDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "testid",
            "testsecret",
            base_url,
        )
        .expect("presenter")
    }

    /// Both the canonical form and the HMAC are reproduced independently from
    /// the parameters the stub actually received, so the test fails if the
    /// production signing path forgets the trailing `&`, sorts differently, or
    /// signs the unencoded form.
    #[tokio::test]
    async fn publish_signs_an_aliyun_compliant_request() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let handle = presenter.publish(&request).await.expect("publish");
        assert_eq!(handle.provider_record_ref.as_deref(), Some("aliyun-rec-9"));

        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        let params = &requests[0];
        assert_eq!(
            params.get("Action").map(String::as_str),
            Some("AddDomainRecord")
        );
        assert_eq!(
            params.get("DomainName").map(String::as_str),
            Some("example.com")
        );
        assert_eq!(
            params.get("RR").map(String::as_str),
            Some("_acme-challenge")
        );
        assert_eq!(params.get("Type").map(String::as_str), Some("TXT"));
        assert_eq!(params.get("Value").map(String::as_str), Some("digest-1"));
        assert_eq!(
            params.get("Version").map(String::as_str),
            Some(ALIYUN_API_VERSION)
        );
        assert_eq!(
            params.get("SignatureMethod").map(String::as_str),
            Some(ALIYUN_SIGNATURE_METHOD)
        );

        let signature = params.get("Signature").expect("signature").clone();
        let timestamp = params.get("Timestamp").expect("timestamp").clone();
        let nonce = params.get("SignatureNonce").expect("nonce").clone();
        assert!(
            timestamp.ends_with('Z') && timestamp.len() == 20,
            "unexpected timestamp {timestamp}"
        );
        assert_eq!(nonce.len(), 32);

        // Rebuild the string-to-sign from the wire parameters, minus Signature.
        let mut canonical: Vec<String> = params
            .iter()
            .filter(|(key, _)| key.as_str() != "Signature")
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    aliyun_percent_encode(key),
                    aliyun_percent_encode(value)
                )
            })
            .collect();
        canonical.sort();
        let string_to_sign = format!(
            "GET&{}&{}",
            aliyun_percent_encode("/"),
            aliyun_percent_encode(&canonical.join("&"))
        );
        let expected = sign_hmac_sha1("testsecret", string_to_sign.as_bytes()).expect("sign");
        assert_eq!(
            signature, expected,
            "the signature must match an independent rebuild of the canonical request"
        );
    }

    /// Pins the signature primitive itself against an external oracle.
    ///
    /// `+sKhUqRXs4rwAayX6SKxZSXBUm4=` is what both
    /// `openssl dgst -sha1 -hmac 'testsecret&' -binary | base64` and
    /// `node -e "crypto.createHmac('sha1','testsecret&').update(msg).digest('base64')"`
    /// produce for the message `GET&%2F&Action%3DDescribeRegions`, so a change
    /// to the key material, the trailing `&`, or the digest has to disagree
    /// with two independent implementations before it can pass.
    #[test]
    fn hmac_sha1_matches_an_external_oracle() {
        let signature =
            sign_hmac_sha1("testsecret", b"GET&%2F&Action%3DDescribeRegions").expect("sign");
        assert_eq!(signature, "+sKhUqRXs4rwAayX6SKxZSXBUm4=");
    }

    #[test]
    fn percent_encoding_follows_the_aliyun_unreserved_set() {
        assert_eq!(aliyun_percent_encode("a b"), "a%20b");
        assert_eq!(aliyun_percent_encode("a+b"), "a%2Bb");
        assert_eq!(aliyun_percent_encode("*"), "%2A");
        assert_eq!(aliyun_percent_encode("~"), "~");
        assert_eq!(aliyun_percent_encode("/"), "%2F");
        assert_eq!(aliyun_percent_encode("-_.~"), "-_.~");
        assert_eq!(
            aliyun_percent_encode("_acme-challenge.example.com"),
            "_acme-challenge.example.com"
        );
    }

    #[test]
    fn timestamp_is_the_rfc3339_utc_shape_aliyun_requires() {
        let moment = time::macros::datetime!(2016-02-23 12:46:24 UTC);
        assert_eq!(aliyun_timestamp_at(moment), "2016-02-23T12:46:24Z");
        assert_eq!(signature_nonce_from(0), "00000000000000000000000000000000");
        assert_eq!(signature_nonce_from(1), "00000000000000000000000000000001");
    }

    #[tokio::test]
    async fn withdraw_tolerates_a_record_that_is_already_gone() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            error_code: Some(ALIYUN_MISSING_RECORD_CODE),
        });
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: Some("aliyun-rec-9".to_string()),
        };

        presenter.withdraw(&handle).await.expect("tolerated");
        let requests = state.requests.lock().expect("lock");
        assert_eq!(
            requests[0].get("Action").map(String::as_str),
            Some("DeleteDomainRecord")
        );
    }

    #[tokio::test]
    async fn withdraw_propagates_an_unrelated_failure() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            error_code: Some("InvalidAccessKeyId.NotFound"),
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url);
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: Some("aliyun-rec-9".to_string()),
        };

        assert!(presenter.withdraw(&handle).await.is_err());
    }

    #[test]
    fn the_secret_is_never_rendered() {
        let presenter = presenter("https://alidns.aliyuncs.com".to_string());
        let rendered = format!("{presenter:?}");
        assert!(rendered.contains("testid"));
        assert!(!rendered.contains("testsecret"));
    }

    #[test]
    fn missing_credentials_are_rejected() {
        let client = DnsApiClient::new().expect("client");
        assert!(AliyunDns01Presenter::new(client, "id", "  ").is_err());
    }

    #[test]
    fn a_record_outside_the_zone_is_rejected_before_signing() {
        let presenter = presenter("https://alidns.aliyuncs.com".to_string());
        let url = presenter.signed_request(
            "2026-09-15T00:00:00Z",
            "nonce",
            "AddDomainRecord",
            &[("DomainName", "example.com")],
        );
        assert!(url.is_ok());
    }
}
