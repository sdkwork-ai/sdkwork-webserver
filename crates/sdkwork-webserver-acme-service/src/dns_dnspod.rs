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
    ACME_CHALLENGE_LABEL, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest,
    DnsAccountVerification, DnsProviderKind, DnsRecordChange, DnsZoneRecord, absolute_record_name,
    normalize_dns_name,
};
use crate::dns_http::{DnsApiClient, form_request};
use crate::{AcmeServiceError, AcmeServiceResult};

pub const DNSPOD_DEFAULT_BASE_URL: &str = "https://dnsapi.cn";

/// TXT records for validation do not need a long TTL.
const CHALLENGE_TTL_SECONDS: u32 = 600;

/// DNSPod signals success with the string code `1`.
const DNSPOD_SUCCESS_CODE: &str = "1";

/// DNSPod line id `0` is the default line and avoids sending a non-ASCII
/// `record_line` value through the form encoder.
const DNSPOD_DEFAULT_LINE_ID: &str = "0";

/// TTL a `Record.Create`/`Record.Modify` without an explicit one carries.
/// DNSPod requires the field; 600 is the vendor console's own default.
const DNSPOD_DEFAULT_TTL: &str = "600";

/// The parameter DNSPod reads for the language of its own error messages.
const DNSPOD_ERROR_LANGUAGE_PARAMETER: &str = "lang";

/// The language those messages are asked for in. See `call`.
const DNSPOD_ERROR_LANGUAGE: &str = "cn";

/// Rows one `Record.List` page may carry. DNSPod's documented `length` ceiling
/// is 3000; one page covers nearly every hosted zone, and the loop below keeps
/// walking for the zones that do not fit.
const DNSPOD_RECORD_PAGE_ROWS: usize = 3000;

/// Completeness bound of one zone's record inventory: the same fail-loud rule
/// the Aliyun inventory read applies, because a silently truncated snapshot
/// would make the resolution page agree with a zone that does not exist.
const DNSPOD_RECORD_INVENTORY_LIMIT: usize = 5_000;

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

    /// Reads the zone's complete resolution-record inventory.
    ///
    /// `Record.List` without the challenge filter the duplicate lookup applies:
    /// every row the provider holds, carried through with DNSPod's own type
    /// spelling. DNSPod names the apex `@` and a wildcard owner `*`, so the
    /// absolute owner is rebuilt against the queried zone exactly as the
    /// publish path builds its challenge name.
    pub async fn zone_records(&self, zone_apex: &str) -> AcmeServiceResult<Vec<DnsZoneRecord>> {
        let zone = normalize_dns_name(zone_apex, "zone apex")?;
        let length = DNSPOD_RECORD_PAGE_ROWS.to_string();
        let mut records = Vec::new();
        let mut offset = 0_usize;
        loop {
            let offset_value = offset.to_string();
            let body = self
                .call(
                    "Record.List",
                    vec![
                        ("domain", zone.as_str()),
                        ("length", length.as_str()),
                        ("offset", offset_value.as_str()),
                    ],
                )
                .await?;
            let fetched = body.records.len();
            for record in body.records {
                let Some(relative) = record.name.filter(|value| !value.is_empty()) else {
                    continue;
                };
                let Some(value) = record.value.filter(|value| !value.is_empty()) else {
                    continue;
                };
                let Some(record_type) = record.record_type.filter(|value| !value.is_empty()) else {
                    continue;
                };
                records.push(DnsZoneRecord {
                    record_name: absolute_record_name(&relative, &zone),
                    record_type,
                    record_value: value,
                    ttl_seconds: record
                        .ttl
                        .and_then(|ttl| ttl.value())
                        .and_then(|ttl| u32::try_from(ttl).ok()),
                    priority: record
                        .mx
                        .and_then(|mx| mx.value())
                        .and_then(|value| u32::try_from(value).ok()),
                    record_line: record.line.filter(|line| !line.is_empty()),
                    provider_record_ref: record.id.filter(|id| !id.is_empty()),
                });
                if records.len() >= DNSPOD_RECORD_INVENTORY_LIMIT {
                    return Err(AcmeServiceError::provider(format!(
                        "the zone's record inventory exceeds the \
                         {DNSPOD_RECORD_INVENTORY_LIMIT}-record snapshot limit"
                    )));
                }
            }
            if fetched < DNSPOD_RECORD_PAGE_ROWS {
                return Ok(records);
            }
            offset += fetched;
        }
    }

    /// One `Record.Create`/`Record.Modify` call, answered with the vendor's
    /// record id. DNSPod's replace-in-place update takes the same fields a
    /// create does, so both actions route through here.
    async fn record_write_call(
        &self,
        action: &str,
        zone: &str,
        record_ref: Option<&str>,
        change: &DnsRecordChange,
    ) -> AcmeServiceResult<String> {
        let ttl = change
            .ttl_seconds
            .map(|value| value.to_string())
            .unwrap_or_else(|| DNSPOD_DEFAULT_TTL.to_string());
        let mx = change.priority.map(|value| value.to_string());
        let mut fields: Vec<(&str, &str)> = Vec::with_capacity(9);
        if let Some(record_ref) = record_ref {
            fields.push(("record_id", record_ref));
        }
        fields.push(("domain", zone));
        fields.push(("sub_domain", change.relative_owner.as_str()));
        fields.push(("record_type", change.record_type.as_str()));
        fields.push(("value", change.record_value.as_str()));
        fields.push(("ttl", ttl.as_str()));
        if let Some(line) = change.record_line.as_deref() {
            fields.push(("record_line", line));
        } else {
            fields.push(("record_line_id", DNSPOD_DEFAULT_LINE_ID));
        }
        if let Some(mx) = mx.as_deref() {
            fields.push(("mx", mx));
        }
        let body = self.call(action, fields).await?;
        body.record
            .map(|record| record.id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AcmeServiceError::provider(format!(
                    "DNSPod accepted {action} without a record id"
                ))
            })
    }

    /// The snapshot row a completed write produces.
    fn change_to_record(zone: &str, change: &DnsRecordChange, record_id: &str) -> DnsZoneRecord {
        DnsZoneRecord {
            record_name: change.absolute_record_name(zone),
            record_type: change.record_type.clone(),
            record_value: change.record_value.clone(),
            ttl_seconds: change.ttl_seconds,
            priority: change.priority,
            record_line: change.record_line.clone(),
            provider_record_ref: Some(record_id.to_owned()),
        }
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

    /// The zone's record inventory, walked to completeness under the same
    /// fail-loud bound the Aliyun inventory read applies.
    async fn list_zone_records(&self, zone_apex: &str) -> AcmeServiceResult<Vec<DnsZoneRecord>> {
        self.zone_records(zone_apex).await
    }

    /// `Record.Create`. The line travels as a line *name* when the change
    /// names one (`默认`, `电信`, …) and as DNSPod's default line id `0`
    /// otherwise — the vendor accepts either spelling and only one of the two
    /// parameters may be sent.
    async fn create_record(
        &self,
        zone_apex: &str,
        change: &DnsRecordChange,
    ) -> AcmeServiceResult<DnsZoneRecord> {
        let zone = normalize_dns_name(zone_apex, "zone apex")?;
        let record_id = self
            .record_write_call("Record.Create", &zone, None, change)
            .await?;
        Ok(Self::change_to_record(&zone, change, &record_id))
    }

    /// `Record.Modify`, addressed by DNSPod's own record id.
    async fn update_record(
        &self,
        zone_apex: &str,
        record_ref: &str,
        change: &DnsRecordChange,
    ) -> AcmeServiceResult<DnsZoneRecord> {
        let zone = normalize_dns_name(zone_apex, "zone apex")?;
        let record_id = self
            .record_write_call("Record.Modify", &zone, Some(record_ref), change)
            .await?;
        Ok(Self::change_to_record(&zone, change, &record_id))
    }

    /// `Record.Remove`, keyed on (domain, record_id); a record that is
    /// already gone is the end state the caller asked for.
    async fn delete_record(&self, zone_apex: &str, record_ref: &str) -> AcmeServiceResult<()> {
        match self
            .call(
                "Record.Remove",
                vec![("domain", zone_apex), ("record_id", record_ref)],
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if is_missing_record(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// `Record.Status` flips one record between enable and disable — DNSPod's
    /// paused-record semantics.
    async fn set_record_status(
        &self,
        zone_apex: &str,
        record_ref: &str,
        enabled: bool,
    ) -> AcmeServiceResult<DnsZoneRecord> {
        let status = if enabled { "enable" } else { "disable" };
        let body = self
            .call(
                "Record.Status",
                vec![
                    ("domain", zone_apex),
                    ("record_id", record_ref),
                    ("status", status),
                ],
            )
            .await?;
        let _ = body;
        // The status response carries only the vendor's acknowledgement; the
        // caller holds the row it flipped, so an identity-only row reports
        // success.
        Ok(DnsZoneRecord {
            record_name: String::new(),
            record_type: String::new(),
            record_value: String::new(),
            ttl_seconds: None,
            priority: None,
            record_line: None,
            provider_record_ref: Some(record_ref.to_owned()),
        })
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

/// DNSPod answers numbers as JSON strings (`"ttl": "600"`), so the inventory
/// parse accepts either spelling: a hard `i64` binding would fail the whole
/// snapshot on the vendor's own documented shape.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DnspodNumber {
    Number(i64),
    Text(String),
}

impl DnspodNumber {
    fn value(&self) -> Option<i64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Text(text) => text.trim().parse().ok(),
        }
    }
}

/// One row of a `Record.List` page. DNSPod names the owner `name` and keeps it
/// sub-domain relative to the queried zone; `mx` carries the priority of the
/// record types that have one, and `line` names the resolution line the row
/// answers on (the default line is spelled in Chinese).
#[derive(Debug, Deserialize)]
struct DnspodListRecord {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(rename = "type", default)]
    record_type: Option<String>,
    #[serde(default)]
    ttl: Option<DnspodNumber>,
    #[serde(default)]
    mx: Option<DnspodNumber>,
    #[serde(default)]
    line: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Form, Json, Router};
    use serde_json::{Value, json};

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

        assert!(
            presenter
                .verify_account("example.com")
                .await
                .expect("verified")
                .is_verified()
        );
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
mod record_inventory_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Form, Json, Router};
    use serde_json::{Value, json};

    /// Answers one page of mixed rows and verifies the inventory carries the
    /// provider's own spellings with the owner rebuilt against the zone.
    #[tokio::test]
    async fn the_inventory_carries_every_record_type_with_absolute_owners() {
        async fn record_list(
            State(state): State<Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .lock()
                .expect("lock")
                .push(("Record.List".into(), fields));
            Json(json!({
                "status": { "code": "1", "message": "ok" },
                "info": { "record_total": 3 },
                "records": [
                    { "id": "rec-a", "name": "www", "type": "A", "value": "203.0.113.10",
                      "ttl": "600", "line": "默认" },
                    { "id": "rec-mx", "name": "@", "type": "MX", "value": "mail.example.com",
                      "ttl": "600", "mx": "10", "line": "默认" },
                    { "id": "rec-star", "name": "*", "type": "CNAME", "value": "example.com",
                      "ttl": "600", "line": "默认" }
                ]
            }))
        }

        let captured: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/Record.List", post(record_list))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let presenter = DnspodDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "12345",
            "api-token",
            format!("http://{address}"),
        )
        .expect("presenter");

        let records = presenter
            .zone_records("example.com")
            .await
            .expect("inventory");
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].record_name, "www.example.com");
        assert_eq!(records[0].record_type, "A");
        assert_eq!(records[0].record_value, "203.0.113.10");
        assert_eq!(records[0].ttl_seconds, Some(600));
        assert_eq!(records[0].record_line.as_deref(), Some("默认"));
        // The apex and wildcard rows keep their DNSPod meanings at the
        // absolute form.
        assert_eq!(records[1].record_name, "example.com");
        assert_eq!(records[1].priority, Some(10));
        assert_eq!(records[2].record_name, "*.example.com");

        let requests = captured.lock().expect("lock");
        let fields = &requests[0].1;
        let get = |key: &str| {
            fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        // No challenge filter on the inventory read: it asks for every row.
        assert_eq!(get("domain").as_deref(), Some("example.com"));
        assert_eq!(get("sub_domain"), None);
        assert!(get("length").is_some());
    }
}

#[cfg(test)]
mod duplicate_publish_tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};

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

#[cfg(test)]
mod record_write_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Form, Json, Router};
    use serde_json::{json, Value};

    #[tokio::test]
    async fn create_sends_sub_domain_line_and_mx_and_status_flips() {
        async fn record_create(
            State(state): State<Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .lock()
                .expect("lock")
                .push(("Record.Create".to_owned(), fields));
            Json(json!({
                "status": { "code": "1", "message": "ok" },
                "record": { "id": "dnspod-new-1" }
            }))
        }

        async fn record_status(
            State(state): State<Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>>,
            Form(fields): Form<Vec<(String, String)>>,
        ) -> Json<Value> {
            state
                .lock()
                .expect("lock")
                .push(("Record.Status".to_owned(), fields));
            Json(json!({
                "status": { "code": "1", "message": "ok" }
            }))
        }

        let captured: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/Record.Create", post(record_create))
            .route("/Record.Status", post(record_status))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let presenter = DnspodDns01Presenter::with_base_url(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            "12345",
            "token",
            format!("http://{address}"),
        )
        .expect("presenter");

        let change = DnsRecordChange::new("MX", "@", "mail.example.com", Some(600), Some(10), None)
            .expect("change");
        let record = presenter
            .create_record("example.com", &change)
            .await
            .expect("create");
        assert_eq!(record.record_name, "example.com");
        assert_eq!(record.provider_record_ref.as_deref(), Some("dnspod-new-1"));

        presenter
            .set_record_status("example.com", "dnspod-new-1", false)
            .await
            .expect("pause");

        let requests = captured.lock().expect("lock");
        let (create_action, create_fields) = &requests[0];
        assert_eq!(create_action, "Record.Create");
        let get = |fields: &[(String, String)], key: &str| {
            fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(get(create_fields, "domain").as_deref(), Some("example.com"));
        assert_eq!(get(create_fields, "sub_domain").as_deref(), Some("@"));
        assert_eq!(get(create_fields, "record_type").as_deref(), Some("MX"));
        assert_eq!(get(create_fields, "value").as_deref(), Some("mail.example.com"));
        assert_eq!(get(create_fields, "ttl").as_deref(), Some("600"));
        assert_eq!(get(create_fields, "record_line_id").as_deref(), Some("0"));
        assert_eq!(get(create_fields, "mx").as_deref(), Some("10"));

        let (status_action, status_fields) = &requests[1];
        assert_eq!(status_action, "Record.Status");
        assert_eq!(get(status_fields, "record_id").as_deref(), Some("dnspod-new-1"));
        assert_eq!(get(status_fields, "status").as_deref(), Some("disable"));
    }
}
