//! DNSPod (dnsapi.cn) adapter.
//!
//! DNSPod is a form-encoded API that answers `HTTP 200` even when it rejects a
//! request, so the transport status alone is not a success signal: the vendor's
//! own `status.code` must be checked too. Records are addressed by the zone
//! apex plus the zone-relative owner, which is why the presenter asks the
//! caller for the apex it selected rather than guessing a public suffix.

use async_trait::async_trait;
use http::Method;
use serde::Deserialize;

use crate::dns::{
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind, ACME_CHALLENGE_LABEL,
};
use crate::dns_http::{form_request, DnsApiClient};
use crate::{AcmeServiceError, AcmeServiceResult};

pub const DNSPOD_DEFAULT_BASE_URL: &str = "https://dnsapi.cn";

/// TXT records for validation do not need a long TTL.
const CHALLENGE_TTL_SECONDS: u32 = 600;

/// DNSPod signals success with the string code `1`.
const DNSPOD_SUCCESS_CODE: &str = "1";

/// DNSPod line id `0` is the default line and avoids sending a non-ASCII
/// `record_line` value through the form encoder.
const DNSPOD_DEFAULT_LINE_ID: &str = "0";

/// DNSPod-backed DNS-01 presenter.
pub struct DnspodDns01Presenter {
    client: DnsApiClient,
    login_token: String,
    base_url: String,
}

impl DnspodDns01Presenter {
    /// `login_id` and `api_token` are the two halves of the DNSPod API token;
    /// they are joined into the `login_token` login parameter per request.
    pub fn new(
        client: DnsApiClient,
        login_id: impl AsRef<str>,
        api_token: impl AsRef<str>,
    ) -> AcmeServiceResult<Self> {
        Self::with_base_url(client, login_id, api_token, DNSPOD_DEFAULT_BASE_URL)
    }

    pub(crate) fn with_base_url(
        client: DnsApiClient,
        login_id: impl AsRef<str>,
        api_token: impl AsRef<str>,
        base_url: impl Into<String>,
    ) -> AcmeServiceResult<Self> {
        let login_id = login_id.as_ref().trim();
        let api_token = api_token.as_ref().trim();
        if login_id.is_empty() || api_token.is_empty() {
            return Err(AcmeServiceError::config(
                "DNSPod login id and API token must both be non-empty",
            ));
        }
        Ok(Self {
            client,
            login_token: format!("{login_id},{api_token}"),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    fn endpoint(&self, action: &str) -> String {
        format!("{}/{action}", self.base_url)
    }

    async fn call(
        &self,
        action: &str,
        mut fields: Vec<(&str, &str)>,
    ) -> AcmeServiceResult<DnspodResponse> {
        fields.push(("login_token", self.login_token.as_str()));
        fields.push(("format", "json"));
        fields.push(("lang", "en"));
        fields.push(("error_on_empty", "no"));
        let request = form_request(
            DnsProviderKind::Dnspod,
            Method::POST,
            &self.endpoint(action),
            &fields,
        )?;
        let response = self.client.send(request).await?;
        response.ensure_success(DnsProviderKind::Dnspod)?;
        let body: DnspodResponse = response.json(DnsProviderKind::Dnspod)?;
        if !body.is_success() {
            return Err(AcmeServiceError::provider(format!(
                "DNSPod rejected {action}: {} ({})",
                body.status.message, body.status.code
            )));
        }
        Ok(body)
    }
}

#[async_trait]
impl Dns01Presenter for DnspodDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::Dnspod)
    }

    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let relative = request.relative_record_name()?;
        let ttl = CHALLENGE_TTL_SECONDS.to_string();
        let record_type = "TXT";
        let body = self
            .call(
                "Record.Create",
                vec![
                    ("domain", request.zone_apex.as_str()),
                    ("sub_domain", relative.as_str()),
                    ("record_type", record_type),
                    ("record_line_id", DNSPOD_DEFAULT_LINE_ID),
                    ("value", request.record_value.as_str()),
                    ("ttl", ttl.as_str()),
                ],
            )
            .await?;
        let record_ref = body
            .record
            .map(|record| record.id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AcmeServiceError::provider("DNSPod accepted the presentation without a record id")
            })?;
        tracing::debug!(
            record_name = %request.record_name,
            "presented {ACME_CHALLENGE_LABEL} TXT record through DNSPod"
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
        // DNSPod reports "record does not exist" as an ordinary error code, and
        // that is the desired end state for a withdrawal, so it is tolerated
        // rather than treated as a failure.
        match self
            .call(
                "Record.Remove",
                vec![
                    ("domain", handle.zone_apex.as_str()),
                    ("record_id", record_ref),
                ],
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if is_missing_record(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// DNSPod error codes that mean the record is already absent.
const DNSPOD_MISSING_RECORD_CODES: [&str; 2] = ["-15", "-16"];

fn is_missing_record(error: &AcmeServiceError) -> bool {
    let AcmeServiceError::Provider(message) = error else {
        return false;
    };
    DNSPOD_MISSING_RECORD_CODES
        .iter()
        .any(|code| message.contains(&format!("({code})")))
}

#[derive(Debug, Deserialize)]
struct DnspodResponse {
    status: DnspodStatus,
    #[serde(default)]
    record: Option<DnspodRecord>,
}

impl DnspodResponse {
    fn is_success(&self) -> bool {
        self.status.code == DNSPOD_SUCCESS_CODE
    }
}

#[derive(Debug, Deserialize)]
struct DnspodStatus {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct DnspodRecord {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Form, Json, Router};
    use serde_json::{json, Value};

    /// One captured call: the action name and the form fields it carried.
    type CapturedCall = (String, Vec<(String, String)>);

    #[derive(Default)]
    struct StubState {
        requests: Mutex<Vec<CapturedCall>>,
        reject_with_code: Option<&'static str>,
        remove_error_code: Option<&'static str>,
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        async fn record_create(
            State(state): State<Arc<StubState>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("Record.Create".into(), fields));
            if let Some(code) = state.reject_with_code {
                return Json(json!({
                    "status": { "code": code, "message": "rejected" },
                }));
            }
            Json(json!({
                "status": { "code": "1", "message": "Action completed successful" },
                "record": { "id": "rec-7", "name": "_acme-challenge", "value": "digest-1" },
            }))
        }

        async fn record_remove(
            State(state): State<Arc<StubState>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("Record.Remove".into(), fields));
            match state.remove_error_code {
                Some(code) => Json(json!({
                    "status": { "code": code, "message": "Domain record is not exists" },
                })),
                None => Json(json!({
                    "status": { "code": "1", "message": "Action completed successful" },
                })),
            }
        }

        let app = Router::new()
            .route("/Record.Create", post(record_create))
            .route("/Record.Remove", post(record_remove))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{address}")
    }

    fn presenter(base_url: String) -> DnspodDns01Presenter {
        DnspodDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "12345",
            "api-token",
            base_url,
        )
        .expect("presenter")
    }

    #[tokio::test]
    async fn publish_sends_the_zone_relative_owner_and_returns_the_record_id() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.sub.example.com", "digest-1")
                .expect("request");

        let handle = presenter.publish(&request).await.expect("publish");
        assert_eq!(handle.provider_record_ref.as_deref(), Some("rec-7"));

        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "Record.Create");
        let fields = &requests[0].1;
        let get = |key: &str| {
            fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(get("domain").as_deref(), Some("example.com"));
        // The owner is reduced against the selected zone, not the registrable
        // domain, so a delegated sub-zone still addresses the right record.
        assert_eq!(get("sub_domain").as_deref(), Some("_acme-challenge.sub"));
        assert_eq!(get("record_type").as_deref(), Some("TXT"));
        assert_eq!(get("value").as_deref(), Some("digest-1"));
        assert_eq!(get("record_line_id").as_deref(), Some("0"));
        assert_eq!(get("login_token").as_deref(), Some("12345,api-token"));
        assert_eq!(get("format").as_deref(), Some("json"));
    }

    #[tokio::test]
    async fn an_http_200_with_a_vendor_error_code_is_a_failure() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            reject_with_code: Some("-15"),
            remove_error_code: None,
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let message = presenter
            .publish(&request)
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("DNSPod rejected Record.Create"));
        assert!(message.contains("-15"));
    }

    #[tokio::test]
    async fn withdraw_tolerates_an_already_removed_record() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            reject_with_code: None,
            remove_error_code: Some("-15"),
        });
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: Some("rec-7".to_string()),
        };

        presenter.withdraw(&handle).await.expect("tolerated");
        assert_eq!(state.requests.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn withdraw_propagates_a_real_failure() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            reject_with_code: None,
            remove_error_code: Some("-3"),
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url);
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: Some("rec-7".to_string()),
        };

        assert!(presenter.withdraw(&handle).await.is_err());
    }

    #[test]
    fn missing_credentials_are_rejected() {
        let client = DnsApiClient::new().expect("client");
        assert!(DnspodDns01Presenter::new(client, "", "token").is_err());
    }
}
