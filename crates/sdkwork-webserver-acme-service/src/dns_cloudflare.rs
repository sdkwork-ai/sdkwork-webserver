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
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsAccountVerification, DnsProviderKind,
    ACME_CHALLENGE_LABEL,
};
use crate::dns_http::{json_request, DnsApiClient};
use crate::{AcmeServiceError, AcmeServiceResult};

pub const CLOUDFLARE_DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4";

/// TXT records for validation do not need a long TTL, and a short one keeps a
/// withdrawn challenge from lingering in resolver caches.
const CHALLENGE_TTL_SECONDS: u32 = 120;

const MAX_ZONE_LIST_ENTRIES: usize = 50;
// The zone list is always queried with Cloudflare's `name=` filter, which is an
// exact match, so the per-page cap cannot hide the zone the call is looking for
// — no matter how many zones the token can see.

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
        let value = format!("Bearer {}", self.api_token).parse().map_err(|_| {
            AcmeServiceError::Internal("encode Cloudflare bearer token".to_string())
        })?;
        request
            .headers_mut()
            .insert(http::header::AUTHORIZATION, value);
        Ok(request)
    }

    /// Resolves the zone on the control plane's cached reference when it has one,
    /// falling back to a lookup by apex name.
    async fn resolve_zone_id(&self, zone_apex: &str) -> AcmeServiceResult<String> {
        if let Some(zone_ref) = self.zone_ref.as_deref() {
            return Ok(zone_ref.to_string());
        }
        self.lookup_zone_id(zone_apex).await
    }

    /// Asks Cloudflare for the zone by name.
    ///
    /// Deliberately skips the cached reference: a probe or an issuance that
    /// reuses a stored zone id never contacts Cloudflare, so it can neither
    /// detect a revoked token nor report what the token cannot see.
    async fn lookup_zone_id(&self, zone_apex: &str) -> AcmeServiceResult<String> {
        let url = format!(
            "{}/zones?name={}&per_page={MAX_ZONE_LIST_ENTRIES}",
            self.base_url,
            crate::dns_http::encode_form_component(zone_apex)
        );
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

    /// Extracts the provider record id from a create response.
    async fn created_record_ref(
        &self,
        response: crate::dns_http::DnsApiResponse,
    ) -> AcmeServiceResult<String> {
        response.ensure_success(DnsProviderKind::Cloudflare)?;
        let body: RecordResponse = response.json(DnsProviderKind::Cloudflare)?;
        if !body.success {
            return Err(AcmeServiceError::provider(format!(
                "Cloudflare refused the TXT presentation: {}",
                body.first_error_message()
            )));
        }
        body.result
            .map(|record| record.id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AcmeServiceError::provider(
                    "Cloudflare accepted the presentation without a record id",
                )
            })
    }

    /// Reads the zone for one exact duplicate: same TXT owner, same value.
    ///
    /// Both filters are exact on Cloudflare's side, so a hit is this record and
    /// nothing else — a challenge for the same name with a *different* value
    /// (the concurrent-order case the trait protects) is never claimed.
    async fn find_existing_record(
        &self,
        zone_id: &str,
        request: &Dns01RecordRequest,
    ) -> AcmeServiceResult<Option<String>> {
        let url = format!(
            "{}/zones/{zone_id}/dns_records?type=TXT&name={}&content={}&per_page=5",
            self.base_url,
            crate::dns_http::encode_form_component(&request.record_name),
            crate::dns_http::encode_form_component(&request.record_value),
        );
        let http_request = self.authorized_request(Method::GET, &url, None)?;
        let response = self.client.send(http_request).await?;
        response.ensure_success(DnsProviderKind::Cloudflare)?;
        let body: RecordListResponse = response.json(DnsProviderKind::Cloudflare)?;
        if !body.success {
            return Err(AcmeServiceError::provider(format!(
                "Cloudflare refused the duplicate-record lookup: {}",
                body.first_error_message()
            )));
        }
        Ok(body
            .result
            .into_iter()
            .map(|record| record.id)
            .find(|id| !id.is_empty()))
    }

    fn handle(request: &Dns01RecordRequest, record_ref: String) -> Dns01RecordHandle {
        Dns01RecordHandle {
            zone_apex: request.zone_apex.clone(),
            record_name: request.record_name.clone(),
            record_value: request.record_value.clone(),
            provider_record_ref: Some(record_ref),
        }
    }
}

#[async_trait]
impl Dns01Presenter for CloudflareDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::Cloudflare)
    }

    /// A zone lookup is exactly the call that proves a Cloudflare account: it
    /// fails for a malformed, revoked or under-scoped token and succeeds only
    /// when the token regards the zone as its own.
    ///
    /// The refusal is classified, because a caller that probes before it writes
    /// has to know whether this is the operator's mistake or Cloudflare's bad
    /// minute. Cloudflare answers a rejected token with `401`/`403` or its own
    /// `10000` authentication code, and it answers an under-scoped token with a
    /// `200` whose zone list simply omits the apex — all three are fixed by
    /// changing the token. A rate limit, a `5xx` or any code this module has not
    /// been taught stays a provider fault and is never allowed to block an order.
    async fn verify_account(&self, zone_apex: &str) -> AcmeServiceResult<DnsAccountVerification> {
        match self.lookup_zone_id(zone_apex).await {
            Ok(_) => Ok(DnsAccountVerification::Verified),
            Err(error) if cloudflare_refusal_is_authority(&error.to_string()) => {
                Err(AcmeServiceError::config(error.to_string()))
            }
            Err(error) => Err(error),
        }
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
        match self.created_record_ref(response).await {
            Ok(record_ref) => Ok(Self::handle(request, record_ref)),
            // The trait requires that publishing a value the provider already
            // holds must not fail: a lost create response — a timeout that
            // lands after the provider accepted the write, a worker retrying
            // after a crash — would otherwise stack a second TXT beside the
            // first and leave both behind when the withdrawal only takes one
            // of them. An exact read by owner *and* value resolves the
            // duplicate into the record that is already there; anything the
            // read cannot confirm re-raises the original refusal.
            Err(error) => match self.find_existing_record(&zone_id, request).await {
                Ok(Some(record_ref)) => {
                    tracing::debug!(
                        record_name = %request.record_name,
                        "the {ACME_CHALLENGE_LABEL} TXT record already exists at Cloudflare; \
                         reusing it instead of creating a second"
                    );
                    Ok(Self::handle(request, record_ref))
                }
                _ => Err(error),
            },
        }
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        let Some(record_ref) = handle.provider_record_ref.as_deref() else {
            return Ok(());
        };
        let zone_id = self.resolve_zone_id(&handle.zone_apex).await?;
        let url = format!("{}/zones/{zone_id}/dns_records/{record_ref}", self.base_url);
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

/// Whether Cloudflare's refusal of a zone lookup proves the token cannot present
/// for that zone.
///
/// Kept next to the adapter's own error strings rather than in a shared helper:
/// the codes are Cloudflare's, and the "cannot see the selected zone" branch is
/// this module's. Everything else — including codes this module has never seen —
/// is deliberately not authority, so an unread refusal can never block an order.
fn cloudflare_refusal_is_authority(message: &str) -> bool {
    /// Cloudflare's own authentication error code, already pinned by
    /// `a_rejected_token_is_reported_by_its_own_message`.
    const AUTHENTICATION_CODE: &str = "10000";
    crate::dns_http::refusal_is_auth_status(message)
        || crate::dns_http::refusal_carries_code(message, &[AUTHENTICATION_CODE])
        || message.contains("cannot see the selected zone")
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

/// One page of the duplicate-record lookup.
#[derive(Debug, Deserialize)]
struct RecordListResponse {
    success: bool,
    #[serde(default)]
    result: Vec<RecordEntry>,
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

impl FirstErrorMessage for RecordListResponse {
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
    use axum::http::StatusCode;
    use axum::routing::{delete, get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};

    #[derive(Default)]
    struct StubState {
        requests: Mutex<Vec<(String, String, Value)>>,
        zone_ref_missing: bool,
        /// Answers the zone lookup the way Cloudflare answers a bad token.
        token_rejected: bool,
        /// Answers the zone lookup with a refusal that is not about the token.
        /// The triple is `(status, code, message)`, so a rate limit can be
        /// answered the way Cloudflare answers one.
        zone_lookup_refusal: Option<(u16, i64, &'static str)>,
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        async fn list_zones(State(state): State<Arc<StubState>>) -> (StatusCode, Json<Value>) {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("GET".into(), "/zones".into(), json!(null)));
            if state.token_rejected {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "success": false,
                        "errors": [{ "code": 10000, "message": "Authentication error" }],
                        "result": null,
                    })),
                );
            }
            if let Some((status, code, message)) = state.zone_lookup_refusal {
                return (
                    StatusCode::from_u16(status).expect("stub status"),
                    Json(json!({
                        "success": false,
                        "errors": [{ "code": code, "message": message }],
                        "result": null,
                    })),
                );
            }
            if state.zone_ref_missing {
                return (
                    StatusCode::OK,
                    Json(json!({ "success": true, "result": [] })),
                );
            }
            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "result": [{ "id": "zone-1", "name": "example.com" }],
                })),
            )
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
            state.requests.lock().expect("lock").push((
                "DELETE".into(),
                "/dns_records/rec-42".into(),
                json!(null),
            ));
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
            zone_ref_missing: true,
            ..StubState::default()
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

    /// A probe is what turns "the renewal failed five minutes into an order"
    /// into "this token is rejected", so it has to carry Cloudflare's own words.
    #[tokio::test]
    async fn a_rejected_token_is_reported_by_its_own_message() {
        let state = Arc::new(StubState {
            token_rejected: true,
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);

        let message = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("Authentication error"), "{message}");
        assert!(message.contains("10000"), "{message}");
        assert!(message.contains("HTTP 401"), "{message}");
    }

    /// The probe must not take the cached zone reference: a stored id would let
    /// a revoked token pass without Cloudflare ever being asked.
    #[tokio::test]
    async fn the_probe_contacts_the_provider_even_with_a_cached_zone_reference() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url, Some("zone-1".to_string()));

        assert!(presenter
            .verify_account("example.com")
            .await
            .expect("verified")
            .is_verified());
        assert_eq!(
            state.requests.lock().expect("lock").len(),
            1,
            "the lookup must not be skipped"
        );
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
        assert!(
            CloudflareDns01Presenter::new(DnsApiClient::new().expect("client"), "  ", None)
                .is_err()
        );
    }

    /// The refusal this classifies is the whole reason the classification
    /// exists: a revoked token must stop an order before it starts, and it must
    /// be reported as something the operator can fix.
    #[tokio::test]
    async fn a_rejected_token_is_a_configuration_mistake() {
        let state = Arc::new(StubState {
            token_rejected: true,
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);

        let error = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail");
        assert!(
            matches!(error, AcmeServiceError::Config(_)),
            "a refused token is the operator's mistake, not a provider fault: {error:?}"
        );
        // ...and the vendor's own words survive the reclassification.
        assert!(
            error.to_string().contains("Authentication error"),
            "{error}"
        );
    }

    /// A token that authenticates but is scoped to another zone is the same
    /// class of mistake, and Cloudflare reports it as a successful call with an
    /// empty result rather than as an error.
    #[tokio::test]
    async fn a_token_that_cannot_see_the_zone_is_a_configuration_mistake() {
        let state = Arc::new(StubState {
            zone_ref_missing: true,
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);

        let error = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail");
        assert!(matches!(error, AcmeServiceError::Config(_)), "{error:?}");
    }

    /// The counterpart that keeps the classification from swallowing everything:
    /// a rate limit is not a configuration mistake, and an order must not be
    /// failed because Cloudflare was busy.
    #[tokio::test]
    async fn a_rate_limited_zone_lookup_stays_a_provider_fault() {
        let state = Arc::new(StubState {
            zone_lookup_refusal: Some((429, 1000, "Too many requests")),
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);

        let error = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail");
        assert!(
            matches!(error, AcmeServiceError::Provider(_)),
            "a rate limit must not be reported as a configuration mistake: {error:?}"
        );
    }

    /// The publisher keeps the plain provider fault, so `publish`/`withdraw`
    /// behaviour is unchanged by the probe's classification.
    #[tokio::test]
    async fn a_refused_publish_is_still_a_provider_fault() {
        let state = Arc::new(StubState {
            token_rejected: true,
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url, None);
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let error = presenter.publish(&request).await.expect_err("must fail");
        assert!(matches!(error, AcmeServiceError::Provider(_)), "{error:?}");
    }
}

#[cfg(test)]
mod duplicate_publish_tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    /// A create the provider answers with "already exists" resolves the record
    /// by an exact read and comes back as success — the trait's "publish for a
    /// value that already exists must not fail", which is what keeps a lost
    /// create response from stacking a second TXT beside the first.
    #[tokio::test]
    async fn a_duplicate_publish_resolves_the_existing_record() {
        async fn create_duplicate() -> (StatusCode, Json<Value>) {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "errors": [{ "code": 81057, "message": "Record already exists" }]
                })),
            )
        }
        async fn find_duplicate(raw_query: axum::extract::RawQuery) -> Response {
            // The lookup must be exact: a different value at the same owner is
            // a concurrent order's record, never this one.
            let query = raw_query.0.unwrap_or_default();
            if query.contains("content=digest-1") {
                Json(json!({
                    "success": true,
                    "result": [{ "id": "cf-existing" }]
                }))
                .into_response()
            } else {
                Json(json!({ "success": true, "result": [] })).into_response()
            }
        }
        let app = Router::new().route(
            "/zones/z/dns_records",
            post(create_duplicate).get(find_duplicate),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let presenter = CloudflareDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "token",
            Some("z".to_owned()),
            format!("http://{address}"),
        )
        .expect("presenter");
        let request =
            Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-1")
                .expect("request");

        let handle = presenter
            .publish(&request)
            .await
            .expect("a duplicate must not fail the publish");
        assert_eq!(handle.provider_record_ref.as_deref(), Some("cf-existing"));
    }
}
