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
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsAccountVerification, DnsProviderKind,
    ACME_CHALLENGE_LABEL,
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

/// The parameter DNSPod reads for the language of its own error messages.
const DNSPOD_ERROR_LANGUAGE_PARAMETER: &str = "lang";

/// The language those messages are asked for in. See `call`.
const DNSPOD_ERROR_LANGUAGE: &str = "cn";

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
        // Ask for the vendor's own error text in the vendor's own language.
        //
        // DNSPod is a Chinese vendor and documents `cn` as the recommended value
        // for its error language; `en` is machine-translated and produces
        // sentences like "Domain record is not exists". Asking for English also
        // contradicts this crate's stated expectation — `dns_http` asserts that
        // "every Chinese vendor answers a bad credential in Chinese" and exists in
        // part to keep those messages intact. The numeric `status.code` travels
        // alongside either way, so nothing that is diagnostic depends on this.
        fields.push((DNSPOD_ERROR_LANGUAGE_PARAMETER, DNSPOD_ERROR_LANGUAGE));
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

    /// Reads the zone for one exact duplicate: same TXT sub-domain, same value.
    ///
    /// `Record.List` filters by `sub_domain` server-side; the value match is
    /// re-checked here in full, because only owner and value together prove the
    /// duplicate — a challenge for the same name with a different value (the
    /// concurrent-order case the trait protects) is never taken over.
    async fn find_existing_record(
        &self,
        zone_apex: &str,
        relative: &str,
        value: &str,
    ) -> AcmeServiceResult<Option<String>> {
        let body = self
            .call(
                "Record.List",
                vec![
                    ("domain", zone_apex),
                    ("sub_domain", relative),
                    ("record_type", "TXT"),
                ],
            )
            .await?;
        Ok(body
            .records
            .into_iter()
            .filter(|record| {
                record
                    .name
                    .as_deref()
                    .map(|name| name.eq_ignore_ascii_case(relative))
                    .unwrap_or(false)
                    && record.value.as_deref() == Some(value)
            })
            .find_map(|record| record.id.filter(|id| !id.is_empty())))
    }
}

#[async_trait]
impl Dns01Presenter for DnspodDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::Dnspod)
    }

    /// Asks DNSPod what it knows about the zone.
    ///
    /// `Domain.Info` is the cheapest read that answers both halves of the
    /// question: a bad `login_token` is refused before the domain is even
    /// consulted, and a domain the account does not own is refused by name.
    ///
    /// A refusal is classified, because a caller that probes before it writes
    /// has to know whether this is the operator's mistake or DNSPod's bad minute.
    /// DNSPod answers over `HTTP 200` with an ordinary error envelope, so the
    /// envelope code is what decides — and only `-1` decides it, for the reason
    /// in [`DNSPOD_AUTHORITY_CODES`]. Every other refusal stays a provider fault
    /// and can never block an order.
    async fn verify_account(&self, zone_apex: &str) -> AcmeServiceResult<DnsAccountVerification> {
        match self.call("Domain.Info", vec![("domain", zone_apex)]).await {
            Ok(_) => Ok(DnsAccountVerification::Verified),
            Err(error) if dnspod_refusal_is_authority(&error.to_string()) => {
                Err(AcmeServiceError::config(error.to_string()))
            }
            Err(error) => Err(error),
        }
    }

    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let relative = request.relative_record_name()?;
        let ttl = CHALLENGE_TTL_SECONDS.to_string();
        let record_type = "TXT";
        let body = match self
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
            .await
        {
            Ok(body) => body,
            // The trait requires that publishing a value the provider already
            // holds must not fail: a lost create response — a timeout that lands
            // after DNSPod accepted the write, a worker retrying after a crash —
            // would otherwise stack a second TXT beside the first and leave both
            // behind when the withdrawal only takes one of them. An exact read
            // by sub-domain *and* value resolves the duplicate into the record
            // that is already there; anything the read cannot confirm re-raises
            // the original refusal.
            Err(error) => {
                match self
                    .find_existing_record(
                        &request.zone_apex,
                        relative.as_str(),
                        &request.record_value,
                    )
                    .await
                {
                    Ok(Some(record_ref)) => {
                        tracing::debug!(
                            record_name = %request.record_name,
                            "the {ACME_CHALLENGE_LABEL} TXT record already exists at DNSPod; \
                             reusing it instead of creating a second"
                        );
                        return Ok(Dns01RecordHandle {
                            zone_apex: request.zone_apex.clone(),
                            record_name: request.record_name.clone(),
                            record_value: request.record_value.clone(),
                            provider_record_ref: Some(record_ref),
                        });
                    }
                    _ => return Err(error),
                }
            }
        };
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

/// The only DNSPod code whose refusal proves the credential is wrong.
///
/// `-1` is "登陆失败" — DNSPod's own words for a `login_token` it does not
/// accept. Nothing else in the common-return table qualifies, and one entry is
/// actively misleading:
///
/// * **`-2` is "API used too frequently", not a token error.** It is a rate
///   limit, so classifying it as authority would fail a perfectly good account
///   by turning a burst of traffic into a configuration verdict.
/// * `-7` ("no permission to use this API") is a permission, and the write path
///   does not need the permission this read needs.
/// * `-8` ("too many failed logins, account temporarily blocked") is transient
///   even though its cause is a wrong credential.
///
/// The zone-ownership half of the probe is deliberately *not* classified here:
/// this module has no confirmed code for "domain not in this account", and a
/// guess would either block a working account or claim a meaning DNSPod does not
/// document. It stays a provider fault, which is the safe direction.
const DNSPOD_AUTHORITY_CODES: [&str; 1] = ["-1"];

fn dnspod_refusal_is_authority(message: &str) -> bool {
    crate::dns_http::refusal_is_auth_status(message)
        || crate::dns_http::refusal_carries_code(message, &DNSPOD_AUTHORITY_CODES)
}

#[derive(Debug, Deserialize)]
struct DnspodResponse {
    status: DnspodStatus,
    #[serde(default)]
    record: Option<DnspodRecord>,
    /// The list body of a `Record.List` call; absent on every write.
    #[serde(default)]
    records: Vec<DnspodListRecord>,
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

/// One row of a `Record.List` page. DNSPod names the owner `name` and keeps it
/// sub-domain relative to the queried zone.
#[derive(Debug, Deserialize)]
struct DnspodListRecord {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
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
        /// Answers the zone lookup, which is what the account probe calls.
        info_error_code: Option<&'static str>,
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        async fn domain_info(
            State(state): State<Arc<StubState>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .requests
                .lock()
                .expect("lock")
                .push(("Domain.Info".into(), fields));
            match state.info_error_code {
                Some(code) => Json(json!({
                    "status": { "code": code, "message": "Login failed" },
                })),
                None => Json(json!({
                    "status": { "code": "1", "message": "Action completed successful" },
                })),
            }
        }

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
            .route("/Domain.Info", post(domain_info))
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
        // The vendor's own language for the vendor's own error text: see `call`.
        assert_eq!(
            get(DNSPOD_ERROR_LANGUAGE_PARAMETER).as_deref(),
            Some(DNSPOD_ERROR_LANGUAGE)
        );
        assert_eq!(DNSPOD_ERROR_LANGUAGE, "cn", "the vendor recommends `cn`");
    }

    #[tokio::test]
    async fn an_http_200_with_a_vendor_error_code_is_a_failure() {
        let state = Arc::new(StubState {
            reject_with_code: Some("-15"),
            ..StubState::default()
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
            remove_error_code: Some("-15"),
            ..StubState::default()
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
            remove_error_code: Some("-3"),
            ..StubState::default()
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

    /// DNSPod answers a bad `login_token` with an ordinary error envelope over
    /// HTTP 200, so the probe's value depends entirely on that envelope reaching
    /// the operator intact.
    #[tokio::test]
    async fn the_probe_reports_the_dnspod_error_for_a_bad_login_token() {
        let state = Arc::new(StubState {
            info_error_code: Some("-1"),
            ..StubState::default()
        });
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);

        let message = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("Login failed"), "{message}");
        assert!(message.contains("-1"), "{message}");
        assert!(message.contains("Domain.Info"), "{message}");

        let requests = state.requests.lock().expect("lock");
        assert_eq!(requests[0].0, "Domain.Info");
        assert_eq!(
            requests[0]
                .1
                .iter()
                .find(|(name, _)| name == "domain")
                .map(|(_, value)| value.as_str()),
            Some("example.com")
        );
    }

    #[tokio::test]
    async fn an_acceptable_account_verifies() {
        let state = Arc::new(StubState::default());
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter(base_url);

        assert!(presenter
            .verify_account("example.com")
            .await
            .expect("verified")
            .is_verified());
        assert_eq!(state.requests.lock().expect("lock").len(), 1);
    }

    #[test]
    fn missing_credentials_are_rejected() {
        let client = DnsApiClient::new().expect("client");
        assert!(DnspodDns01Presenter::new(client, "", "token").is_err());
    }

    /// `-1` is DNSPod's word for a `login_token` it does not accept, and it
    /// arrives over `HTTP 200`, so the envelope is the only signal.
    #[tokio::test]
    async fn a_refused_login_token_is_a_configuration_mistake() {
        let state = Arc::new(StubState {
            info_error_code: Some("-1"),
            ..StubState::default()
        });
        let base_url = spawn_stub(state).await;
        let presenter = presenter(base_url);

        let error = presenter
            .verify_account("example.com")
            .await
            .expect_err("must fail");
        assert!(
            matches!(error, AcmeServiceError::Config(_)),
            "a refused login token is the operator's mistake: {error:?}"
        );
        assert!(error.to_string().contains("-1"), "{error}");
    }

    /// The trap this module has to avoid: DNSPod's `-2` reads like a token
    /// problem and is not one. It is "API used too frequently", so classifying it
    /// as configuration would fail a working account the first time an operator
    /// retried quickly — turning a rate limit into "your credential is wrong".
    #[tokio::test]
    async fn a_rate_limited_probe_is_not_a_configuration_mistake() {
        for code in ["-2", "-3", "-4", "-7", "-8", "-99"] {
            let state = Arc::new(StubState {
                info_error_code: Some(code),
                ..StubState::default()
            });
            let base_url = spawn_stub(state).await;
            let presenter = presenter(base_url);

            let error = presenter
                .verify_account("example.com")
                .await
                .expect_err("must fail");
            assert!(
                matches!(error, AcmeServiceError::Provider(_)),
                "{code} must stay a provider fault, not a credential verdict: {error:?}"
            );
        }
    }
}

#[cfg(test)]
mod duplicate_publish_tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    /// The vendor's duplicate refusal followed by an exact read must come back
    /// as success carrying the existing record's id. A row whose value differs
    /// is a concurrent order's record and is never claimed.
    #[tokio::test]
    async fn a_duplicate_publish_resolves_the_exact_existing_record() {
        async fn create_record() -> Json<Value> {
            Json(json!({
                "status": { "code": "3", "message": "记录重复" }
            }))
        }
        async fn list_records() -> Json<Value> {
            Json(json!({
                "status": { "code": "1", "message": "ok" },
                "records": [
                    { "id": "dnspod-stale", "name": "_acme-challenge", "value": "digest-0" },
                    { "id": "dnspod-exact", "name": "_acme-challenge", "value": "digest-1" }
                ]
            }))
        }
        let app = Router::new()
            .route("/Record.Create", post(create_record))
            .route("/Record.List", post(list_records));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let presenter = DnspodDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "testid",
            "testtoken",
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
        assert_eq!(handle.provider_record_ref.as_deref(), Some("dnspod-exact"));
    }
}
