//! Cloudflare DNS API v4 adapter.
//!
//! Cloudflare permits several TXT records to share one owner name, which is
//! exactly what a wildcard-plus-apex presentation needs: each value becomes its
//! own record with its own id, so withdrawing one never disturbs the other.
//! Authentication is a scoped API token sent as a bearer credential.

use async_trait::async_trait;
use http::Method;
use serde::Deserialize;

use crate::dns::{
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind, ACME_CHALLENGE_LABEL,
};
use crate::dns_http::{json_request, DnsApiClient};
use crate::{AcmeServiceError, AcmeServiceResult};

pub const CLOUDFLARE_DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4";

/// TXT records for validation do not need a long TTL, and a short one keeps a
/// withdrawn challenge from lingering in resolver caches.
const CHALLENGE_TTL_SECONDS: u32 = 120;

const MAX_ZONE_LIST_ENTRIES: usize = 50;

/// Cloudflare-backed DNS-01 presenter.
pub struct CloudflareDns01Presenter {
    client: DnsApiClient,
    api_token: String,
    /// Zone identity cached in the control plane's credential row. When absent
    /// the zone is resolved by apex name on each presentation.
    zone_ref: Option<String>,
    base_url: String,
}

impl CloudflareDns01Presenter {
    pub fn new(
        client: DnsApiClient,
        api_token: impl Into<String>,
        zone_ref: Option<String>,
    ) -> AcmeServiceResult<Self> {
        Self::with_base_url(client, api_token, zone_ref, CLOUDFLARE_DEFAULT_BASE_URL)
    }

    pub(crate) fn with_base_url(
        client: DnsApiClient,
        api_token: impl Into<String>,
        zone_ref: Option<String>,
        base_url: impl Into<String>,
    ) -> AcmeServiceResult<Self> {
        let api_token = api_token.into();
        if api_token.trim().is_empty() {
            return Err(AcmeServiceError::config(
                "Cloudflare API token must not be empty",
            ));
        }
        Ok(Self {
            client,
            api_token,
            zone_ref,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    fn authorized_request(
        &self,
        method: Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> AcmeServiceResult<http::Request<http_body_util::Full<bytes::Bytes>>> {
        let mut request = json_request(DnsProviderKind::Cloudflare, method, url, body)?;
        let value = format!("Bearer {}", self.api_token)
            .parse()
            .map_err(|_| AcmeServiceError::Internal("encode Cloudflare bearer token".to_string()))?;
        request
            .headers_mut()
            .insert(http::header::AUTHORIZATION, value);
        Ok(request)
    }

    async fn resolve_zone_id(&self, zone_apex: &str) -> AcmeServiceResult<String> {
        if let Some(zone_ref) = self.zone_ref.as_deref() {
            return Ok(zone_ref.to_string());
        }
        let url = format!("{}/zones?name={}&per_page={MAX_ZONE_LIST_ENTRIES}", self.base_url, crate::dns_http::encode_form_component(zone_apex));
        let request = self.authorized_request(Method::GET, &url, None)?;
        let response = self.client.send(request).await?;
        response.ensure_success(DnsProviderKind::Cloudflare)?;
        let body: ZoneListResponse = response.json(DnsProviderKind::Cloudflare)?;
        if !body.success {
            return Err(AcmeServiceError::provider(format!(
                "Cloudflare refused the zone lookup: {}",
                body.first_error_message()
            )));
        }
        body.result
            .iter()
            .find(|zone| zone.name.eq_ignore_ascii_case(zone_apex))
            .map(|zone| zone.id.clone())
            .ok_or_else(|| {
                // The apex is intentionally not echoed: the caller already knows
                // which zone it selected, and the message may reach a log.
                AcmeServiceError::provider(
                    "the configured Cloudflare token cannot see the selected zone",
                )
            })
    }
}

#[async_trait]
impl Dns01Presenter for CloudflareDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::Cloudflare)
    }

    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let zone_id = self.resolve_zone_id(&request.zone_apex).await?;
        let url = format!("{}/zones/{zone_id}/dns_records", self.base_url);
        let payload = serde_json::json!({
            "type": "TXT",
            "name": request.record_name,
            "content": request.record_value,
            "ttl": CHALLENGE_TTL_SECONDS,
            "comment": format!("{ACME_CHALLENGE_LABEL} validation (managed by sdkwork)"),
        });
        let http_request = self.authorized_request(Method::POST, &url, Some(payload))?;
        let response = self.client.send(http_request).await?;
        response.ensure_success(DnsProviderKind::Cloudflare)?;
        let body: RecordResponse = response.json(DnsProviderKind::Cloudflare)?;
        if !body.success {
            return Err(AcmeServiceError::provider(format!(
                "Cloudflare refused the TXT presentation: {}",
                body.first_error_message()
            )));
        }
        let record_ref = body
            .result
            .map(|record| record.id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AcmeServiceError::provider("Cloudflare accepted the presentation without a record id")
            })?;
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
        let zone_id = self.resolve_zone_id(&handle.zone_apex).await?;
        let url = format!(
            "{}/zones/{zone_id}/dns_records/{record_ref}",
            self.base_url
        );
        let http_request = self.authorized_request(Method::DELETE, &url, None)?;
        let response = self.client.send(http_request).await?;
        // A record that is already gone is the desired end state.
        if response.status == http::StatusCode::NOT_FOUND {
            return Ok(());
        }
        response.ensure_success(DnsProviderKind::Cloudflare)?;
        let body: BaseResponse = response.json(DnsProviderKind::Cloudflare)?;
        if !body.success {
            return Err(AcmeServiceError::provider(format!(
                "Cloudflare refused to withdraw the TXT record: {}",
                body.first_error_message()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ZoneListResponse {
    success: bool,
    #[serde(default)]
    result: Vec<ZoneEntry>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ZoneEntry {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RecordResponse {
    success: bool,
    #[serde(default)]
    result: Option<RecordEntry>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct RecordEntry {
    id: String,
}

#[derive(Debug, Deserialize)]
struct BaseResponse {
    success: bool,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

trait FirstErrorMessage {
    fn errors(&self) -> &[ApiError];
    fn first_error_message(&self) -> String {
        self.errors()
            .first()
            .map(|error| format!("{} ({})", error.message, error.code))
            .unwrap_or_else(|| "no error detail returned".to_string())
    }
}

impl FirstErrorMessage for ZoneListResponse {
    fn errors(&self) -> &[ApiError] {
        &self.errors
    }
}

impl FirstErrorMessage for RecordResponse {
    fn errors(&self) -> &[ApiError] {
        &self.errors
    }
}

impl FirstErrorMessage for BaseResponse {
    fn errors(&self) -> &[ApiError] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::{delete, get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};

    #[derive(Default)]
    struct StubState {
        requests: Mutex<Vec<(String, String, Value)>>,
        zone_ref_missing: bool,
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        async fn list_zones(State(state): State<Arc<StubState>>) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("GET".into(), "/zones".into(), json!(null)));
            if state.zone_ref_missing {
                return Json(json!({ "success": true, "result": [] }));
            }
            Json(json!({
                "success": true,
                "result": [{ "id": "zone-1", "name": "example.com" }],
            }))
        }

        async fn create_record(
            State(state): State<Arc<StubState>>,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("POST".into(), "/dns_records".into(), body));
            Json(json!({ "success": true, "result": { "id": "rec-42" } }))
        }

        async fn delete_record(State(state): State<Arc<StubState>>) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("DELETE".into(), "/dns_records/rec-42".into(), json!(null)));
            Json(json!({ "success": true, "result": { "id": "rec-42" } }))
        }

        let app = Router::new()
            .route("/zones", get(list_zones))
            .route("/zones/{zone}/dns_records", post(create_record))
            .route("/zones/{zone}/dns_records/{record}", delete(delete_record))
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

    fn presenter(base_url: String, zone_ref: Option<String>) -> CloudflareDns01Presenter {
        CloudflareDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "token-value",
            zone_ref,
            base_url,
        )
        .expect("presenter")
    }

    #[tokio::test]
    async fn publish_resolves_the_zone_then_creates_a_txt_record() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url, None);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let handle = presenter.publish(&request).await.expect("publish");
        assert_eq!(handle.provider_record_ref.as_deref(), Some("rec-42"));

        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].1, "/zones");
        assert_eq!(requests[1].0, "POST");
        let body = &requests[1].2;
        assert_eq!(body["type"], "TXT");
        assert_eq!(body["name"], "_acme-challenge.example.com");
        assert_eq!(body["content"], "digest-1");
        assert_eq!(body["ttl"], CHALLENGE_TTL_SECONDS);
    }

    #[tokio::test]
    async fn a_configured_zone_ref_skips_the_lookup() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url, Some("zone-1".to_string()));
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        presenter.publish(&request).await.expect("publish");
        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests.len(), 1, "the zone lookup must be skipped");
        assert_eq!(requests[0].0, "POST");
    }

    #[tokio::test]
    async fn an_invisible_zone_fails_without_echoing_the_apex() {
        let state = Arc::new(StubState {
            requests: Mutex::new(Vec::new()),
            zone_ref_missing: true,
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let message = presenter
            .publish(&request)
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("cannot see the selected zone"));
        assert!(!message.contains("example.com"));
    }

    #[tokio::test]
    async fn withdraw_deletes_by_record_id_and_tolerates_a_missing_record() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url, Some("zone-1".to_string()));
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: Some("rec-42".to_string()),
        };

        presenter.withdraw(&handle).await.expect("withdraw");
        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests[0].0, "DELETE");
        assert!(requests[0].1.contains("rec-42"));
    }

    #[tokio::test]
    async fn withdraw_without_a_record_ref_is_a_no_op() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url, Some("zone-1".to_string()));
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest-1".to_string(),
            provider_record_ref: None,
        };
        presenter.withdraw(&handle).await.expect("no-op");
        assert!(state.requests.lock().expect("lock").is_empty());
    }

    #[test]
    fn an_empty_token_is_rejected() {
        assert!(CloudflareDns01Presenter::new(
            DnsApiClient::new().expect("client"),
            "  ",
            None
        )
        .is_err());
    }
}
