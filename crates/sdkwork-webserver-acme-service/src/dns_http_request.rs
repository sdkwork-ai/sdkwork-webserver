//! Template-driven DNS provider adapter: the family that closes the set.
//!
//! Every other adapter in this crate is one vendor's HTTP API written out as
//! Rust. This one carries the requests as *data*, so a vendor with an HTTPS API
//! is configured rather than coded, and "is provider X supported?" stops being a
//! question about the release you are running. It is the same escape hatch
//! `lego` ships as `--dns httpreq`, and it is what makes an open-ended provider
//! list honest rather than aspirational.
//!
//! # The configuration
//!
//! One JSON object, stored as the account's secret payload:
//!
//! ```json
//! {
//!   "baseUrl": "https://api.example.com/v1",
//!   "vars": { "apiToken": "…" },
//!   "headers": { "Authorization": "Bearer {{var:apiToken}}" },
//!   "publish": {
//!     "method": "POST",
//!     "url": "zones/{{zoneApex}}/records",
//!     "contentType": "application/json",
//!     "body": "{\"type\":\"TXT\",\"name\":\"{{relativeName}}\",\"data\":\"{{recordValue}}\",\"ttl\":120}",
//!     "recordRefPath": "result.id"
//!   },
//!   "withdraw": { "method": "DELETE", "url": "zones/{{zoneApex}}/records/{{recordRef}}" }
//! }
//! ```
//!
//! A template may interpolate four record variables — `zoneApex`, `recordName`,
//! `relativeName`, `recordValue` — plus `recordRef` in `withdraw` only, plus
//! `var:NAME` for anything declared under `vars`. Secrets belong in `vars`; a
//! template then carries only their names, which is why a template is safe to
//! keep beside the rest of an account's configuration.
//!
//! # CNAME delegation needs no extra mechanism
//!
//! Pointing `_acme-challenge.example.com` at a CNAME you control, and publishing
//! the TXT there, is the standard answer for a provider with no usable API. It
//! is expressible here as a configuration whose published owner is a *constant*:
//! the account is registered against `example.com` (so zone resolution finds
//! it), while the template publishes into the delegate's zone through the
//! delegate's API. `{{relativeName}}` is then unavailable by design, and saying
//! so is what the error does.
//!
//! # Trust boundary, and what is refused
//!
//! Templates are authored by whoever registers the account, so they are
//! configuration rather than untrusted input — but an obviously wrong one is
//! still refused loudly at registration, because the alternative is an order
//! failing at the CA a minute later:
//!
//! * a non-`https` endpoint (plaintext is reachable only from this crate's
//!   tests, matching [`DnsApiClient`]'s production posture);
//! * a metadata-service host, which is never a DNS API;
//! * an undeclared variable, an empty or unclosed token, `recordRef` outside
//!   `withdraw` (publish cannot have one yet), or a template that never carries
//!   the value it is supposed to publish.
//!
//! No resolved URL, header value or request body is ever rendered into an error:
//! a template may legitimately carry a token in a query string, so echoing the
//! request back is exactly how a credential reaches a log.
//!
//! # Diagnosing a rejected call
//!
//! A provider that rejects a call says why, and that sentence is the only thing
//! that tells a wrong token apart from an unowned zone, so it is carried through
//! to the operator. The message is taken from [`crate::dns_http`]'s extraction of
//! the response body, and `errorPath` names the field when the vendor's envelope
//! is not one of the common shapes (`resultMsg`, `errmsg`, …). Whatever the
//! source, every value declared under `vars` is redacted out of the text before
//! it is reported, because a vendor that echoes the request back would otherwise
//! publish the credential the template carried.

use std::collections::BTreeMap;
use std::fmt;

use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use http::{Method, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::dns::{
    dns_relative_record_name, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest,
    DnsProviderKind,
};
use crate::dns_http::{templated_request, DnsApiClient, DnsApiResponse};
use crate::{AcmeServiceError, AcmeServiceResult};

/// Ceiling for one account's configuration.
///
/// This is the same bound the app-api contract declares as
/// `secretAccessKey.maxLength`, and the DNS provider parity gate asserts the two
/// are equal: the document travels as the account's secret, so a configuration
/// the console accepts must be one this presenter can parse. A few kilobytes is
/// far more than any real provider's templates need.
pub const MAX_HTTP_REQUEST_CONFIG_BYTES: usize = 4096;

/// Prefix that marks an operator-declared variable, where secrets live.
const VARIABLE_PREFIX: &str = "var:";

/// Hosts that are never a DNS API. Every other host is allowed: the endpoint is
/// chosen by whoever registered the account, and a DNS gateway may legitimately
/// live on an internal address.
const BLOCKED_HOSTS: [&str; 6] = [
    "169.254.169.254",
    "100.100.100.200",
    "fd00:ec2::254",
    "metadata.google.internal",
    "metadata.tencentyun.com",
    "instance-data",
];

/// Substituted for each token when an endpoint is checked at construction time,
/// before any record value is known.
const PLACEHOLDER: &str = "template-value";

/// The accepted document shape, spelled out in the parse error.
///
/// A serde message for a mistyped key or a missing field is the most useful thing
/// to an operator and the most dangerous thing to a log — the same message for an
/// invalid *type* quotes the offending value, and this document holds secrets. So
/// the parse error reports the position plus this schema instead of the message.
const CONFIG_SHAPE: &str = "expected keys: baseUrl, vars, headers, publish {url, method, \
                             contentType, body, recordRefPath, errorPath}, withdraw {url, \
                             method, contentType, body, errorPath}";

/// Record variables a template may interpolate, with whether `withdraw` only.
const RECORD_VARIABLES: [&str; 5] = [
    "zoneApex",
    "recordName",
    "relativeName",
    "recordValue",
    "recordRef",
];

/// One templated HTTP call.
///
/// Deliberately not `Debug`: a step carries the operator's body template, and a
/// `Debug` rendering is how a template ends up verbatim in a log line.
struct RequestStep {
    method: Method,
    url: String,
    content_type: Option<String>,
    body: Option<String>,
    /// Dot path into the publish response yielding the provider's record
    /// identity, when the provider assigns one.
    record_ref_path: Option<String>,
    /// Dot path into a *failed* response yielding the vendor's diagnostic, for
    /// envelopes that are not one of the common shapes.
    error_path: Option<String>,
}

/// The validated form of an operator's configuration.
///
/// Deliberately not `Debug`, for the same reason as [`RequestStep`]: `vars` is
/// where the secrets are.
struct ResolvedConfig {
    base_url: Option<String>,
    vars: BTreeMap<String, String>,
    headers: Vec<(HeaderName, String)>,
    publish: RequestStep,
    withdraw: RequestStep,
}

/// A DNS provider driven by request templates held in the account's secret.
pub struct HttpRequestDns01Presenter {
    client: DnsApiClient,
    config: ResolvedConfig,
    /// Whether a plaintext endpoint is tolerated. `false` everywhere except
    /// this crate's tests, which point the presenter at a stub server.
    allow_plaintext: bool,
}

impl fmt::Debug for HttpRequestDns01Presenter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The configuration holds the endpoint templates and whatever `vars`
        // declares, so only the family is rendered.
        formatter.write_str("HttpRequestDns01Presenter")
    }
}

impl HttpRequestDns01Presenter {
    /// Builds the presenter from one account's configuration document.
    pub fn new(client: DnsApiClient, config: &str) -> AcmeServiceResult<Self> {
        Self::from_config(client, config, false)
    }

    /// Test entry point: same configuration, plaintext endpoints permitted.
    #[cfg(test)]
    pub(crate) fn allowing_plaintext(
        client: DnsApiClient,
        config: &str,
    ) -> AcmeServiceResult<Self> {
        Self::from_config(client, config, true)
    }

    pub(crate) fn from_config(
        client: DnsApiClient,
        config: &str,
        allow_plaintext: bool,
    ) -> AcmeServiceResult<Self> {
        if config.len() > MAX_HTTP_REQUEST_CONFIG_BYTES {
            return Err(AcmeServiceError::config(format!(
                "the DNS request configuration is {} bytes, over the \
                 {MAX_HTTP_REQUEST_CONFIG_BYTES}-byte ceiling",
                config.len()
            )));
        }
        // Only the position and the schema are reported: a serde message for an
        // invalid type quotes the offending value, and this document holds
        // secrets.
        let raw: RawConfig = serde_json::from_str(config).map_err(|error| {
            AcmeServiceError::config(format!(
                "the DNS request configuration is not the expected JSON object (line {}, column {}); {CONFIG_SHAPE}",
                error.line(),
                error.column()
            ))
        })?;

        let publish = resolve_step("publish", raw.publish, Method::POST)?;
        let withdraw = resolve_step("withdraw", raw.withdraw, Method::DELETE)?;

        let mut headers = Vec::with_capacity(raw.headers.len());
        for (name, template) in raw.headers {
            let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                AcmeServiceError::config(
                    "a DNS request header name is not a valid HTTP header name",
                )
            })?;
            // A header applies to both steps, so the stricter rule applies:
            // `publish` has not seen a record reference, so no header may
            // interpolate one.
            for token in template_tokens(&template)? {
                assert_token_is_known(&token, &raw.vars, false)?;
            }
            headers.push((header_name, template));
        }

        for (label, step, record_ref_allowed) in
            [("publish", &publish, false), ("withdraw", &withdraw, true)]
        {
            for template in [Some(step.url.as_str()), step.body.as_deref()] {
                let Some(template) = template else { continue };
                for token in template_tokens(template)? {
                    assert_token_is_known(&token, &raw.vars, record_ref_allowed)?;
                }
            }
            // The url is checked with every token replaced, so a bad scheme or a
            // baked-in metadata address fails at registration rather than on the
            // first order. The label only shapes the message.
            assert_endpoint_is_allowed(
                &substitute_placeholders(&step.url)?,
                raw.base_url.as_deref(),
                allow_plaintext,
            )
            .map_err(|error| AcmeServiceError::config(format!("`{label}`: {error}")))?;
        }

        let carries_value = publish.url.contains("recordValue")
            || publish
                .body
                .as_deref()
                .is_some_and(|body| body.contains("recordValue"))
            || headers
                .iter()
                .any(|(_, template)| template.contains("recordValue"));
        if !carries_value {
            return Err(AcmeServiceError::config(
                "the DNS request `publish` template never carries `{{recordValue}}`, so the \
                 challenge could never be validated",
            ));
        }

        Ok(Self {
            client,
            config: ResolvedConfig {
                base_url: raw
                    .base_url
                    .map(|base| base.trim().trim_end_matches('/').to_string()),
                vars: raw.vars,
                headers,
                publish,
                withdraw,
            },
            allow_plaintext,
        })
    }

    /// Interpolates, sends and bounds one step.
    async fn execute(
        &self,
        step: &RequestStep,
        scope: &TemplateScope<'_>,
    ) -> AcmeServiceResult<DnsApiResponse> {
        let target = interpolate(&step.url, scope)?;
        let url = resolve_endpoint(self.config.base_url.as_deref(), &target)?;
        // Re-checked per call: only now is the real host known.
        assert_endpoint_is_allowed(&url, None, self.allow_plaintext)?;
        let body = match step.body.as_deref() {
            Some(template) => Some(interpolate(template, scope)?),
            None => None,
        };
        let mut request = templated_request(
            DnsProviderKind::HttpRequest,
            step.method.clone(),
            &url,
            step.content_type.as_deref(),
            body.as_deref(),
        )?;
        for (name, template) in &self.config.headers {
            let resolved = interpolate(template, scope)?;
            let value = HeaderValue::from_str(&resolved).map_err(|_| {
                // Not echoed: a header value is exactly where a secret lives.
                AcmeServiceError::config(format!(
                    "the DNS request header `{name}` resolved to a value that is not a valid \
                     HTTP header value"
                ))
            })?;
            request.headers_mut().insert(name.clone(), value);
        }
        self.client.send(request).await
    }

    /// Every value declared under `vars`, which is what a redaction pass needs.
    fn secret_values(&self) -> Vec<String> {
        self.config.vars.values().cloned().collect()
    }

    /// Fails when the provider rejected one step, reporting the provider's own
    /// diagnostic.
    ///
    /// The order is deliberate: the operator-declared `errorPath` first, because
    /// it is exact; then [`DnsApiResponse::ensure_success_redacting`], which
    /// mines the common envelope shapes; then a bounded excerpt of the body. All
    /// three paths run the declared secrets through redaction, because the one
    /// thing that must never appear in a certificate failure is the credential
    /// whose rejection the message is explaining.
    fn ensure_step_succeeded(
        &self,
        step: &RequestStep,
        response: &DnsApiResponse,
    ) -> AcmeServiceResult<()> {
        if response.is_success() {
            return Ok(());
        }
        let secrets = self.secret_values();
        if let Some(path) = step.error_path.as_deref() {
            if let Ok(body) = response.json::<Value>(DnsProviderKind::HttpRequest) {
                if let Some(detail) = json_path_lookup(&body, path) {
                    let detail = crate::dns_http::redact_declared_secrets(&detail, &secrets);
                    return Err(AcmeServiceError::provider(format!(
                        "HTTP_REQUEST rejected the request with HTTP {}: {detail}",
                        response.status.as_u16()
                    )));
                }
            }
        }
        response.ensure_success_redacting(DnsProviderKind::HttpRequest, &secrets)
    }
}

#[async_trait]
impl Dns01Presenter for HttpRequestDns01Presenter {
    fn provider_kind(&self) -> Option<DnsProviderKind> {
        Some(DnsProviderKind::HttpRequest)
    }

    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        let scope = TemplateScope::for_request(request, &self.config.vars);
        let response = self.execute(&self.config.publish, &scope).await?;
        // A record that is already there is the desired end state, so a conflict
        // is success. This is what keeps presentation idempotent when the order
        // state machine retries a call whose response it never saw.
        let already_present = response.status == StatusCode::CONFLICT;
        if !already_present {
            self.ensure_step_succeeded(&self.config.publish, &response)?;
        }
        let record_ref = match self.config.publish.record_ref_path.as_deref() {
            None => None,
            // A conflict body belongs to the provider's error shape, so a missing
            // reference there is not an error: withdrawal then has to address the
            // record by name and value.
            Some(path) if already_present => response
                .json::<Value>(DnsProviderKind::HttpRequest)
                .ok()
                .and_then(|body| json_path_lookup(&body, path)),
            Some(path) => {
                let body: Value = response.json(DnsProviderKind::HttpRequest)?;
                Some(json_path_lookup(&body, path).ok_or_else(|| {
                    AcmeServiceError::provider(format!(
                        "the DNS provider accepted the presentation without `{path}` in its response"
                    ))
                })?)
            }
        };
        Ok(Dns01RecordHandle {
            zone_apex: request.zone_apex.clone(),
            record_name: request.record_name.clone(),
            record_value: request.record_value.clone(),
            provider_record_ref: record_ref,
        })
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        let step = &self.config.withdraw;
        if step_addresses_by_reference(step) && handle.provider_record_ref.is_none() {
            return Err(AcmeServiceError::provider(
                "the DNS provider returned no record reference and the `withdraw` template \
                 addresses the record by `{{recordRef}}`; address it by name and value instead",
            ));
        }
        let scope = TemplateScope::for_handle(handle, &self.config.vars);
        let response = self.execute(step, &scope).await?;
        // Already gone is the desired end state.
        if response.status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        self.ensure_step_succeeded(step, &response)
    }
}

/// Whether a step interpolates the provider's record identity.
fn step_addresses_by_reference(step: &RequestStep) -> bool {
    step.url.contains("recordRef")
        || step
            .body
            .as_deref()
            .is_some_and(|body| body.contains("recordRef"))
}

/// The values one template may interpolate.
struct TemplateScope<'a> {
    zone_apex: &'a str,
    record_name: &'a str,
    record_value: &'a str,
    record_ref: Option<&'a str>,
    /// The owner relative to the zone, when the record is inside it.
    relative_name: Option<String>,
    vars: &'a BTreeMap<String, String>,
}

impl<'a> TemplateScope<'a> {
    fn for_request(request: &'a Dns01RecordRequest, vars: &'a BTreeMap<String, String>) -> Self {
        Self {
            zone_apex: &request.zone_apex,
            record_name: &request.record_name,
            record_value: &request.record_value,
            record_ref: None,
            relative_name: relative_or_none(&request.record_name, &request.zone_apex),
            vars,
        }
    }

    fn for_handle(handle: &'a Dns01RecordHandle, vars: &'a BTreeMap<String, String>) -> Self {
        Self {
            zone_apex: &handle.zone_apex,
            record_name: &handle.record_name,
            record_value: &handle.record_value,
            record_ref: handle.provider_record_ref.as_deref(),
            relative_name: relative_or_none(&handle.record_name, &handle.zone_apex),
            vars,
        }
    }

    fn value(&self, token: &str) -> AcmeServiceResult<String> {
        if let Some(name) = token.strip_prefix(VARIABLE_PREFIX) {
            return self.vars.get(name).cloned().ok_or_else(|| {
                AcmeServiceError::config(format!(
                    "the DNS request template uses `var:{name}`, which the account does not declare"
                ))
            });
        }
        match token {
            "zoneApex" => Ok(self.zone_apex.to_string()),
            "recordName" => Ok(self.record_name.to_string()),
            "recordValue" => Ok(self.record_value.to_string()),
            "recordRef" => Ok(self.record_ref.unwrap_or_default().to_string()),
            "relativeName" => self.relative_name.clone().ok_or_else(|| {
                // Reachable when a template interpolates the owner and the record
                // is outside the zone — which is what a delegated zone looks
                // like, so the message names the fix rather than the symptom.
                AcmeServiceError::config(
                    "the DNS request template uses `{{relativeName}}`, but the challenge record \
                     does not sit inside the configured zone; use `{{recordName}}` and a constant \
                     owner for a delegated zone",
                )
            }),
            other => Err(AcmeServiceError::config(format!(
                "the DNS request template uses unknown variable `{other}`"
            ))),
        }
    }
}

/// The zone-relative owner, or `None` when the record is outside the zone.
///
/// A delegated zone publishes outside its apex on purpose, so this must be a
/// value rather than an error until a template actually asks for it.
fn relative_or_none(record_name: &str, zone_apex: &str) -> Option<String> {
    dns_relative_record_name(record_name, zone_apex).ok()
}

/// Turns one raw step into a resolved one, defaulting the method.
fn resolve_step(
    label: &str,
    raw: RawStep,
    default_method: Method,
) -> AcmeServiceResult<RequestStep> {
    let method = match raw.method.as_deref().map(str::trim) {
        None | Some("") => default_method,
        Some(text) => Method::from_bytes(text.to_ascii_uppercase().as_bytes()).map_err(|_| {
            AcmeServiceError::config(format!(
                "the DNS request `{label}` method is not a valid HTTP method"
            ))
        })?,
    };
    let url = raw.url.trim().to_string();
    if url.is_empty() {
        return Err(AcmeServiceError::config(format!(
            "the DNS request `{label}` has an empty url"
        )));
    }
    // Both path fields are validated the same way: an empty segment can never
    // resolve, so accepting it would turn a typo into a silent "the provider
    // said nothing" at issuance time instead of a configuration error now.
    let record_ref_path = validated_json_path(label, "recordRefPath", raw.record_ref_path)?;
    let error_path = validated_json_path(label, "errorPath", raw.error_path)?;
    Ok(RequestStep {
        method,
        url,
        content_type: raw
            .content_type
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        body: raw.body,
        record_ref_path,
        error_path,
    })
}

/// Trims an optional `label.field` JSON path and refuses one that cannot resolve.
fn validated_json_path(
    label: &str,
    field: &str,
    value: Option<String>,
) -> AcmeServiceResult<Option<String>> {
    let Some(path) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if path.contains("{{") {
        return Err(AcmeServiceError::config(format!(
            "the DNS request `{label}` {field} is a literal JSON path, not a template"
        )));
    }
    if path.split('.').any(str::is_empty) {
        return Err(AcmeServiceError::config(format!(
            "the DNS request `{label}` {field} `{path}` has an empty segment"
        )));
    }
    Ok(Some(path))
}

/// Every `{{…}}` token in a template, in order.
fn template_tokens(template: &str) -> AcmeServiceResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // The template is not echoed: it may carry a secret.
            return Err(AcmeServiceError::config(
                "a DNS request template has an unclosed `{{`",
            ));
        };
        let token = after[..end].trim();
        if token.is_empty() {
            return Err(AcmeServiceError::config(
                "a DNS request template has an empty `{{}}`",
            ));
        }
        tokens.push(token.to_string());
        rest = &after[end + 2..];
    }
    Ok(tokens)
}

/// Rejects a token the runtime could not resolve.
fn assert_token_is_known(
    token: &str,
    vars: &BTreeMap<String, String>,
    record_ref_allowed: bool,
) -> AcmeServiceResult<()> {
    if let Some(name) = token.strip_prefix(VARIABLE_PREFIX) {
        if vars.contains_key(name) {
            return Ok(());
        }
        return Err(AcmeServiceError::config(format!(
            "the DNS request template uses `var:{name}`, which `vars` does not declare"
        )));
    }
    if !RECORD_VARIABLES.contains(&token) {
        return Err(AcmeServiceError::config(format!(
            "the DNS request template uses unknown variable `{other}`; valid: {variables}, var:NAME",
            other = token,
            variables = RECORD_VARIABLES.join(", ")
        )));
    }
    if token == "recordRef" && !record_ref_allowed {
        return Err(AcmeServiceError::config(
            "the DNS request template uses `recordRef` outside `withdraw`; only the withdraw step \
             knows the provider's record identity",
        ));
    }
    Ok(())
}

/// Replaces every token with a placeholder, so URL validation can run before any
/// record is known.
fn substitute_placeholders(template: &str) -> AcmeServiceResult<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(AcmeServiceError::config(
                "a DNS request template has an unclosed `{{`",
            ));
        };
        output.push_str(PLACEHOLDER);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

/// Expands a template against the values available in this call.
fn interpolate(template: &str, scope: &TemplateScope<'_>) -> AcmeServiceResult<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(AcmeServiceError::config(
                "a DNS request template has an unclosed `{{`",
            ));
        };
        output.push_str(&scope.value(after[..end].trim())?);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

/// Joins a step's target with the account's base url.
fn resolve_endpoint(base_url: Option<&str>, target: &str) -> AcmeServiceResult<String> {
    let target = target.trim();
    if target.starts_with("http://") || target.starts_with("https://") {
        return Ok(target.to_string());
    }
    if target.starts_with("//") {
        return Err(AcmeServiceError::config(
            "a DNS request url may not be protocol-relative",
        ));
    }
    let Some(base) = base_url.filter(|base| !base.is_empty()) else {
        return Err(AcmeServiceError::config(
            "a relative DNS request url needs `baseUrl` on the account",
        ));
    };
    Ok(format!(
        "{}/{}",
        base.trim_end_matches('/'),
        target.trim_start_matches('/')
    ))
}

/// Rejects the endpoints this presenter must never call.
fn assert_endpoint_is_allowed(
    url: &str,
    base_url: Option<&str>,
    allow_plaintext: bool,
) -> AcmeServiceResult<()> {
    let resolved = resolve_endpoint(base_url, url)?;
    let uri: http::Uri = resolved.parse().map_err(|_| {
        // Not echoed: a url may carry a token in its query string.
        AcmeServiceError::config("a DNS request url is not a valid URL")
    })?;
    match uri.scheme_str() {
        Some("https") => {}
        Some("http") if allow_plaintext => {}
        Some("http") => {
            return Err(AcmeServiceError::config(
                "a DNS request url must use https; plaintext endpoints are reachable only from \
                 this crate's tests",
            ))
        }
        _ => {
            return Err(AcmeServiceError::config(
                "a DNS request url must be an http(s) URL",
            ))
        }
    }
    let host = uri
        .host()
        .ok_or_else(|| AcmeServiceError::config("a DNS request url has no host"))?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    let blocked = BLOCKED_HOSTS
        .iter()
        .any(|blocked| host == *blocked || host.ends_with(&format!(".{blocked}")));
    if blocked {
        return Err(AcmeServiceError::config(
            "a DNS request url points at an instance metadata service, which is never a DNS API",
        ));
    }
    Ok(())
}

/// Reads a dot path (`result.id`, `items.0.id`) out of a JSON response.
fn json_path_lookup(value: &Value, path: &str) -> Option<String> {
    let mut current = value;
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    match current {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    publish: RawStep,
    withdraw: RawStep,
}

/// One step as written by an operator.
///
/// Unknown fields are refused rather than ignored: a mistyped key in a
/// credential document is otherwise a silent no-op, and the failure would only
/// appear at the CA.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    #[serde(default)]
    method: Option<String>,
    url: String,
    #[serde(default, rename = "contentType")]
    content_type: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default, rename = "recordRefPath")]
    record_ref_path: Option<String>,
    /// Dot path into a failed response yielding the vendor's diagnostic. Needed
    /// only for an envelope outside the common shapes.
    #[serde(default, rename = "errorPath")]
    error_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::{delete, post};
    use axum::{Json, Router};
    use serde_json::json;

    #[derive(Default)]
    struct StubState {
        calls: Mutex<Vec<StubCall>>,
        publish_status: Option<u16>,
        withdraw_status: Option<u16>,
        /// Raw body sent with a failing publish, for vendors whose envelope is
        /// not one of the common shapes.
        publish_error_body: Option<String>,
        /// Answers a failing publish by quoting the request back, the way a
        /// provider that validates a signature in the body does.
        echo_request_on_error: bool,
    }

    struct StubCall {
        method: String,
        target: String,
        authorization: Option<String>,
        body: String,
    }

    async fn publish(
        State(state): State<Arc<StubState>>,
        headers: HeaderMap,
        body: String,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        state.calls.lock().expect("lock").push(StubCall {
            method: "POST".into(),
            target: "/v1/records".into(),
            authorization: headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            body: body.clone(),
        });
        // A duplicate is answered the way a provider answers it: an error body
        // that carries no record identity.
        let status = StatusCode::from_u16(state.publish_status.unwrap_or(200)).expect("status");
        if status == StatusCode::CONFLICT {
            return (status, Json(json!({ "error": "already exists" }))).into_response();
        }
        if !status.is_success() {
            if let Some(raw) = state.publish_error_body.as_deref() {
                return (status, raw.to_string()).into_response();
            }
            if state.echo_request_on_error {
                return (
                    status,
                    Json(json!({ "message": format!("signature check failed for {body}") })),
                )
                    .into_response();
            }
        }
        (status, Json(json!({ "result": { "id": "rec-7" } }))).into_response()
    }

    async fn withdraw(State(state): State<Arc<StubState>>) -> StatusCode {
        state.calls.lock().expect("lock").push(StubCall {
            method: "DELETE".into(),
            target: "/v1/records/rec-7".into(),
            authorization: None,
            body: String::new(),
        });
        StatusCode::from_u16(state.withdraw_status.unwrap_or(200)).expect("status")
    }

    async fn spawn_stub(state: Arc<StubState>) -> String {
        let app = Router::new()
            .route("/v1/records", post(publish))
            .route("/v1/records/rec-7", delete(withdraw))
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

    fn config(base_url: &str) -> String {
        format!(
            r#"{{
              "baseUrl": "{base_url}/v1",
              "vars": {{ "apiToken": "token-from-vars" }},
              "headers": {{ "Authorization": "Bearer {{{{var:apiToken}}}}" }},
              "publish": {{
                "url": "records",
                "contentType": "application/json",
                "body": "{{\"type\":\"TXT\",\"name\":\"{{{{relativeName}}}}\",\"data\":\"{{{{recordValue}}}}\"}}",
                "recordRefPath": "result.id"
              }},
              "withdraw": {{ "url": "records/{{{{recordRef}}}}" }}
            }}"#
        )
    }

    fn presenter(state: &Arc<StubState>, base_url: &str) -> HttpRequestDns01Presenter {
        let _ = state;
        HttpRequestDns01Presenter::allowing_plaintext(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            &config(base_url),
        )
        .expect("presenter")
    }

    /// The shared fixture, with the publish step declaring where a failing
    /// provider puts its diagnostic.
    fn presenter_with_error_path(
        base_url: &str,
        error_path: Option<&str>,
    ) -> HttpRequestDns01Presenter {
        let error_path = error_path
            .map(|path| format!(",\n                \"errorPath\": \"{path}\""))
            .unwrap_or_default();
        let config = format!(
            r#"{{
              "baseUrl": "{base_url}/v1",
              "vars": {{ "apiToken": "token-from-vars" }},
              "headers": {{ "Authorization": "Bearer {{{{var:apiToken}}}}" }},
              "publish": {{
                "url": "records",
                "contentType": "application/json",
                "body": "{{\"type\":\"TXT\",\"name\":\"{{{{relativeName}}}}\",\"data\":\"{{{{recordValue}}}}\"}}",
                "recordRefPath": "result.id"{error_path}
              }},
              "withdraw": {{ "url": "records/{{{{recordRef}}}}" }}
            }}"#
        );
        HttpRequestDns01Presenter::allowing_plaintext(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            &config,
        )
        .expect("presenter")
    }

    fn request() -> Dns01RecordRequest {
        Dns01RecordRequest::new("example.com", "_acme-challenge.example.com", "digest-value")
            .expect("request")
    }

    async fn stub(
        publish_status: Option<u16>,
        withdraw_status: Option<u16>,
    ) -> (Arc<StubState>, String) {
        let state = Arc::new(StubState {
            publish_status,
            withdraw_status,
            ..StubState::default()
        });
        let base_url = spawn_stub(state.clone()).await;
        (state, base_url)
    }

    #[tokio::test]
    async fn publishes_through_the_operator_template() {
        let (state, base_url) = stub(None, None).await;
        let presenter = presenter(&state, &base_url);

        let handle = presenter.publish(&request()).await.expect("publish");
        assert_eq!(handle.provider_record_ref.as_deref(), Some("rec-7"));

        let calls = state.calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "POST");
        assert_eq!(calls[0].target, "/v1/records");
        assert_eq!(
            calls[0].authorization.as_deref(),
            Some("Bearer token-from-vars")
        );
        let body: Value = serde_json::from_str(&calls[0].body).expect("json body");
        assert_eq!(body["name"], "_acme-challenge");
        assert_eq!(body["data"], "digest-value");
        assert_eq!(body["type"], "TXT");
    }

    #[tokio::test]
    async fn withdraws_by_the_reference_captured_at_publish() {
        let (state, base_url) = stub(None, None).await;
        let presenter = presenter(&state, &base_url);
        let handle = presenter.publish(&request()).await.expect("publish");

        presenter.withdraw(&handle).await.expect("withdraw");
        let calls = state.calls.lock().expect("lock");
        let last = calls.last().expect("call");
        assert_eq!(last.method, "DELETE");
        assert_eq!(last.target, "/v1/records/rec-7");
    }

    #[tokio::test]
    async fn an_already_present_record_is_not_a_failure() {
        let (state, base_url) = stub(Some(409), None).await;
        let presenter = presenter(&state, &base_url);

        // A retry after a timeout whose response the caller never saw must not
        // fail; the error body carries no identity, so none is recorded.
        let handle = presenter.publish(&request()).await.expect("idempotent");
        assert!(handle.provider_record_ref.is_none());
    }

    #[tokio::test]
    async fn withdrawing_a_vanished_record_is_not_a_failure() {
        let (state, base_url) = stub(None, Some(404)).await;
        let presenter = presenter(&state, &base_url);
        let handle = presenter.publish(&request()).await.expect("publish");

        presenter.withdraw(&handle).await.expect("idempotent");
    }

    #[tokio::test]
    async fn a_rejected_presentation_surfaces_the_provider_status() {
        let (state, base_url) = stub(Some(401), None).await;
        let presenter = presenter(&state, &base_url);

        let message = presenter
            .publish(&request())
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("HTTP 401"), "{message}");
    }

    #[test]
    fn a_template_that_never_carries_the_value_is_rejected() {
        let config = r#"{
          "publish": { "url": "https://api.example.com/records", "body": "{\"type\":\"TXT\"}" },
          "withdraw": { "url": "https://api.example.com/records" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("recordValue"), "{message}");
    }

    #[test]
    fn an_undeclared_variable_is_rejected() {
        let config = r#"{
          "headers": { "Authorization": "Bearer {{var:missing}}" },
          "publish": { "url": "https://api.example.com/r", "body": "{{recordValue}}" },
          "withdraw": { "url": "https://api.example.com/r" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("does not declare"), "{message}");
    }

    #[test]
    fn publishing_cannot_use_the_record_reference() {
        let config = r#"{
          "publish": { "url": "https://api.example.com/r/{{recordRef}}", "body": "{{recordValue}}" },
          "withdraw": { "url": "https://api.example.com/r/{{recordRef}}" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("recordRef"), "{message}");
    }

    #[test]
    fn a_metadata_endpoint_is_rejected() {
        let config = r#"{
          "publish": { "url": "https://169.254.169.254/latest/meta-data", "body": "{{recordValue}}" },
          "withdraw": { "url": "https://api.example.com/r" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("metadata"), "{message}");
    }

    #[test]
    fn a_plaintext_endpoint_is_rejected_outside_tests() {
        let config = r#"{
          "publish": { "url": "http://api.example.com/records", "body": "{{recordValue}}" },
          "withdraw": { "url": "http://api.example.com/records" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("https"), "{message}");
    }

    #[test]
    fn a_relative_url_needs_a_base_url() {
        let config = r#"{
          "publish": { "url": "records", "body": "{{recordValue}}" },
          "withdraw": { "url": "records" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("baseUrl"), "{message}");
    }

    #[test]
    fn a_mistyped_key_is_refused_rather_than_ignored() {
        let config = r#"{
          "publish": { "url": "https://api.example.com/r", "body": "{{recordValue}}" },
          "withdraw": { "url": "https://api.example.com/r", "reccordRefPath": "id" }
        }"#;
        let message = HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("line 3"), "{message}");
        // The position alone is not actionable, so the error also spells out the
        // accepted keys — which is the only place a mistyped one can be found.
        assert!(message.contains("recordRefPath"), "{message}");
    }

    #[test]
    fn a_relative_owner_outside_the_zone_fails_with_the_reason() {
        let config = r#"{
          "publish": { "url": "https://api.example.com/records", "body": "{\"name\":\"{{relativeName}}\",\"data\":\"{{recordValue}}\"}" },
          "withdraw": { "url": "https://api.example.com/records" }
        }"#;
        // The configuration itself is accepted without a record in hand: only
        // the owner is unavailable, and that must surface when a template asks
        // for it rather than when the account is registered.
        HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
            .expect("the configuration is valid");
        let vars = BTreeMap::new();
        let scope = TemplateScope {
            zone_apex: "example.com",
            record_name: "_acme-challenge.delegate.test",
            record_value: "digest",
            record_ref: None,
            relative_name: relative_or_none("_acme-challenge.delegate.test", "example.com"),
            vars: &vars,
        };
        let message = interpolate("{{relativeName}}", &scope)
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("delegated zone"), "{message}");
    }

    #[test]
    fn withdrawing_by_reference_without_one_is_refused() {
        let config = r#"{
          "publish": { "url": "https://api.example.com/records", "body": "{{recordValue}}" },
          "withdraw": { "url": "https://api.example.com/records/{{recordRef}}" }
        }"#;
        let presenter =
            HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), config)
                .expect("presenter");
        let handle = Dns01RecordHandle {
            zone_apex: "example.com".to_string(),
            record_name: "_acme-challenge.example.com".to_string(),
            record_value: "digest".to_string(),
            provider_record_ref: None,
        };
        let message = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(presenter.withdraw(&handle))
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("recordRef"), "{message}");
    }

    #[test]
    fn a_dot_path_reads_arrays_and_refuses_non_scalars() {
        let body = json!({ "result": [{ "id": 42 }], "empty": "", "nested": { "id": "" } });
        assert_eq!(
            json_path_lookup(&body, "result.0.id").as_deref(),
            Some("42")
        );
        assert_eq!(json_path_lookup(&body, "empty"), None);
        assert_eq!(json_path_lookup(&body, "nested.id"), None);
        assert_eq!(json_path_lookup(&body, "result.9.id"), None);
    }

    /// A vendor whose envelope is not one of the common shapes (`resultMsg`,
    /// `errmsg`, …) has its diagnostic named by the operator rather than
    /// guessed at, and it reaches the message verbatim.
    #[tokio::test]
    async fn a_declared_error_path_surfaces_the_vendors_own_message() {
        let state = Arc::new(StubState {
            publish_status: Some(403),
            publish_error_body: Some(
                r#"{"resultCode":"FAIL","resultMsg":"Invalid signature"}"#.to_string(),
            ),
            ..StubState::default()
        });
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter_with_error_path(&base_url, Some("resultMsg"));

        let message = presenter
            .publish(&request())
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("Invalid signature"), "{message}");
        assert!(message.contains("HTTP 403"), "{message}");
        assert!(
            !message.contains("resultCode"),
            "the raw envelope was reported instead of the declared path: {message}"
        );
    }

    /// An `errorPath` that does not resolve is not a dead end: the generic
    /// extraction still finds the message, so a slightly stale path degrades to a
    /// useful error rather than to raw JSON.
    #[tokio::test]
    async fn an_error_path_that_does_not_resolve_falls_back_to_the_provider_body() {
        let state = Arc::new(StubState {
            publish_status: Some(401),
            publish_error_body: Some(r#"{"message":"Unauthorized"}"#.to_string()),
            ..StubState::default()
        });
        let base_url = spawn_stub(state.clone()).await;
        let presenter = presenter_with_error_path(&base_url, Some("errors.0.message"));

        let message = presenter
            .publish(&request())
            .await
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("Unauthorized"), "{message}");
    }

    /// The whole point of reporting a rejected credential is to help the
    /// operator replace it, so the credential itself must not travel with the
    /// message — even when the provider quotes the request back.
    #[tokio::test]
    async fn a_declared_secret_is_redacted_out_of_a_provider_error() {
        let state = Arc::new(StubState {
            publish_status: Some(401),
            echo_request_on_error: true,
            ..StubState::default()
        });
        let base_url = spawn_stub(state.clone()).await;
        // The token travels in the body here on purpose: a template may place it
        // anywhere the vendor reads it from.
        let config = format!(
            r#"{{
              "baseUrl": "{base_url}/v1",
              "vars": {{ "apiToken": "token-from-vars" }},
              "publish": {{
                "url": "records",
                "contentType": "application/json",
                "body": "{{\"token\":\"{{{{var:apiToken}}}}\",\"data\":\"{{{{recordValue}}}}\"}}"
              }},
              "withdraw": {{ "url": "records/rec-7" }}
            }}"#
        );
        let presenter = HttpRequestDns01Presenter::allowing_plaintext(
            DnsApiClient::new_allowing_plaintext().expect("client"),
            &config,
        )
        .expect("presenter");

        let message = presenter
            .publish(&request())
            .await
            .expect_err("must fail")
            .to_string();
        assert!(
            !message.contains("token-from-vars"),
            "the credential was echoed back: {message}"
        );
        assert!(message.contains(crate::dns_http::REDACTED), "{message}");
        assert!(
            message.contains("signature check failed"),
            "the diagnostic was lost while redacting: {message}"
        );
    }

    #[test]
    fn an_error_path_is_a_literal_json_path_not_a_template() {
        let message = build_with_error_path("{{resultMsg}}")
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("literal JSON path"), "{message}");
    }

    #[test]
    fn a_json_path_with_an_empty_segment_is_refused() {
        let message = build_with_error_path("result..msg")
            .expect_err("must fail")
            .to_string();
        assert!(message.contains("empty segment"), "{message}");
    }

    fn build_with_error_path(error_path: &str) -> AcmeServiceResult<HttpRequestDns01Presenter> {
        let config = format!(
            r#"{{
              "publish": {{ "url": "https://api.example.com/r", "body": "{{{{recordValue}}}}",
                            "errorPath": "{error_path}" }},
              "withdraw": {{ "url": "https://api.example.com/r" }}
            }}"#
        );
        HttpRequestDns01Presenter::new(DnsApiClient::new().expect("client"), &config)
    }
}
