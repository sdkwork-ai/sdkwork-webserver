use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    future::Future,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path as AxumPath, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::post,
    Router,
};
use sdkwork_drive_contract::drive::events::{
    derive_website_event_channel_id, derive_website_event_verification_token,
    WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID,
};
use sdkwork_utils_rust::{hmac_sha256, secure_compare, sha256_hash};
use sdkwork_webserver_delivery_runtime::{
    parse_website_provider_event, FileWebsiteProviderEventCheckpointStore,
    WebsiteProviderEventInvalidator, WebsiteProviderEventProcessError,
    WebsiteProviderEventProcessor, WebsiteProviderEventReconciler, WebsiteProviderEventSource,
    MAXIMUM_PROVIDER_EVENT_BYTES,
};
use serde::Deserialize;
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::{net::TcpListener, sync::Semaphore};
use zeroize::Zeroizing;

const INGRESS_SCHEMA_VERSION: &str = "sdkwork.website-provider-event-ingress.v1";
const MAXIMUM_INGRESS_CONFIG_BYTES: u64 = 1024 * 1024;
const MAXIMUM_SUBSCRIPTIONS: usize = 1024;
const MAXIMUM_SECRET_BYTES: u64 = 4096;
const DEFAULT_MAXIMUM_CHECKPOINT_STREAMS: usize = 4096;
const DEFAULT_MAXIMUM_CLOCK_SKEW_SECONDS: u64 = 300;
const DEFAULT_MAXIMUM_CONCURRENT_DELIVERIES: usize = 32;
const MINIMUM_CLOCK_SKEW_SECONDS: u64 = 30;
const MAXIMUM_CLOCK_SKEW_SECONDS: u64 = 3600;
const MAXIMUM_CONCURRENT_DELIVERIES: usize = 256;
const EVENT_ID_HEADER: &str = "x-sdkwork-event-id";
const DRIVE_EVENT_TIMESTAMP_HEADER: &str = "x-sdkwork-event-timestamp";
const KNOWLEDGEBASE_EVENT_TIME_HEADER: &str = "x-sdkwork-event-time";
const EVENT_SIGNATURE_HEADER: &str = "x-sdkwork-event-signature";
const EVENT_SEQUENCE_HEADER: &str = "x-sdkwork-event-sequence";
const EVENT_TYPE_HEADER: &str = "x-sdkwork-event-type";
const DRIVE_CHANNEL_ID_HEADER: &str = "x-sdkwork-drive-channel-id";
const DRIVE_RETRY_COUNT_HEADER: &str = "x-sdkwork-event-retry-count";
const DRIVE_IDEMPOTENCY_KEY_HEADER: &str = "x-sdkwork-idempotency-key";
const MAXIMUM_DRIVE_CHANNEL_ID_BYTES: usize = 64;
const MAXIMUM_DRIVE_RETRY_COUNT: u8 = 9;

#[derive(Debug, Error)]
pub(crate) enum WebsiteProviderEventIngressError {
    #[error("website provider event ingress configuration is unavailable or invalid")]
    Config,
    #[error("website provider event ingress listener bind failed")]
    Bind,
    #[error("website provider event ingress listener failed")]
    Serve,
}

pub(crate) struct WebsiteProviderEventIngress {
    listener: TcpListener,
    router: Router,
}

impl WebsiteProviderEventIngress {
    pub(crate) async fn bind_from_file(
        path: &Path,
        expected_tenant_scope_hash: &str,
        expected_node_uuid: &str,
        require_drive: bool,
        require_knowledgebase: bool,
        invalidator: Arc<dyn WebsiteProviderEventInvalidator>,
        reconciler: Arc<dyn WebsiteProviderEventReconciler>,
    ) -> Result<Self, WebsiteProviderEventIngressError> {
        let config = load_config(path)?;
        let validated = validate_config(
            config,
            expected_tenant_scope_hash,
            expected_node_uuid,
            require_drive,
            require_knowledgebase,
        )?;
        let listener = TcpListener::bind(validated.bind_address)
            .await
            .map_err(|_| WebsiteProviderEventIngressError::Bind)?;
        let checkpoints = Arc::new(
            FileWebsiteProviderEventCheckpointStore::open(
                &validated.checkpoint_directory,
                validated.maximum_checkpoint_streams,
            )
            .map_err(|_| WebsiteProviderEventIngressError::Config)?,
        );
        let processor = Arc::new(WebsiteProviderEventProcessor::new(
            checkpoints,
            invalidator,
            reconciler,
        ));
        let state = Arc::new(IngressState {
            subscriptions: validated.subscriptions,
            processor,
            maximum_clock_skew_seconds: validated.maximum_clock_skew_seconds,
            concurrency: Arc::new(Semaphore::new(validated.maximum_concurrent_deliveries)),
        });
        let router = provider_event_router(state);
        Ok(Self { listener, router })
    }

    pub(crate) async fn run_until<F>(
        self,
        shutdown: F,
    ) -> Result<(), WebsiteProviderEventIngressError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|_| WebsiteProviderEventIngressError::Serve)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderEventIngressConfig {
    schema_version: String,
    bind_address: String,
    checkpoint_directory: PathBuf,
    #[serde(default = "default_maximum_checkpoint_streams")]
    maximum_checkpoint_streams: usize,
    #[serde(default = "default_maximum_clock_skew_seconds")]
    maximum_clock_skew_seconds: u64,
    #[serde(default = "default_maximum_concurrent_deliveries")]
    maximum_concurrent_deliveries: usize,
    subscriptions: Vec<ProviderEventSubscriptionConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderEventSubscriptionConfig {
    subscription_id: String,
    provider: ProviderEventSourceConfig,
    tenant_scope_hash: String,
    tenant_id: String,
    organization_id: Option<String>,
    drive_node_uuid: Option<String>,
    secret_file: PathBuf,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProviderEventSourceConfig {
    Drive,
    Knowledgebase,
}

struct ValidatedIngressConfig {
    bind_address: SocketAddr,
    checkpoint_directory: PathBuf,
    maximum_checkpoint_streams: usize,
    maximum_clock_skew_seconds: u64,
    maximum_concurrent_deliveries: usize,
    subscriptions: BTreeMap<String, ProviderEventSubscription>,
}

struct ProviderEventSubscription {
    provider: ProviderEventSourceConfig,
    tenant_id: String,
    organization_id: Option<String>,
    drive_node_uuid: Option<String>,
    signing_secret: Zeroizing<Vec<u8>>,
}

struct IngressState {
    subscriptions: BTreeMap<String, ProviderEventSubscription>,
    processor: Arc<WebsiteProviderEventProcessor>,
    maximum_clock_skew_seconds: u64,
    concurrency: Arc<Semaphore>,
}

fn default_maximum_checkpoint_streams() -> usize {
    DEFAULT_MAXIMUM_CHECKPOINT_STREAMS
}

fn default_maximum_clock_skew_seconds() -> u64 {
    DEFAULT_MAXIMUM_CLOCK_SKEW_SECONDS
}

fn default_maximum_concurrent_deliveries() -> usize {
    DEFAULT_MAXIMUM_CONCURRENT_DELIVERIES
}

fn load_config(
    path: &Path,
) -> Result<ProviderEventIngressConfig, WebsiteProviderEventIngressError> {
    let before = fs::metadata(path).map_err(|_| WebsiteProviderEventIngressError::Config)?;
    if !before.is_file() || before.len() == 0 || before.len() > MAXIMUM_INGRESS_CONFIG_BYTES {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    let bytes = fs::read(path).map_err(|_| WebsiteProviderEventIngressError::Config)?;
    let after = fs::metadata(path).map_err(|_| WebsiteProviderEventIngressError::Config)?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() as u64 != after.len()
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    serde_json::from_slice(&bytes).map_err(|_| WebsiteProviderEventIngressError::Config)
}

fn validate_config(
    config: ProviderEventIngressConfig,
    expected_tenant_scope_hash: &str,
    expected_node_uuid: &str,
    require_drive: bool,
    require_knowledgebase: bool,
) -> Result<ValidatedIngressConfig, WebsiteProviderEventIngressError> {
    if config.schema_version != INGRESS_SCHEMA_VERSION
        || config.subscriptions.is_empty()
        || config.subscriptions.len() > MAXIMUM_SUBSCRIPTIONS
        || config.checkpoint_directory.as_os_str().is_empty()
        || config.maximum_checkpoint_streams == 0
        || config.maximum_checkpoint_streams > 65_536
        || !(MINIMUM_CLOCK_SKEW_SECONDS..=MAXIMUM_CLOCK_SKEW_SECONDS)
            .contains(&config.maximum_clock_skew_seconds)
        || !(1..=MAXIMUM_CONCURRENT_DELIVERIES).contains(&config.maximum_concurrent_deliveries)
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    let bind_address = config
        .bind_address
        .parse::<SocketAddr>()
        .map_err(|_| WebsiteProviderEventIngressError::Config)?;
    if !bind_address.ip().is_loopback() || bind_address.port() == 0 {
        return Err(WebsiteProviderEventIngressError::Config);
    }

    let mut subscriptions = BTreeMap::new();
    let mut drive_nodes = BTreeSet::new();
    let mut has_drive = false;
    let mut has_knowledgebase = false;
    for subscription in config.subscriptions {
        validate_subscription_id(&subscription.subscription_id)?;
        if subscription.tenant_scope_hash != expected_tenant_scope_hash {
            return Err(WebsiteProviderEventIngressError::Config);
        }
        validate_bounded_identity(&subscription.tenant_id, 64)?;
        if let Some(value) = subscription.organization_id.as_deref() {
            validate_bounded_identity(value, 64)?;
        }
        let secret = read_secret(&subscription.secret_file, subscription.provider)?;
        match subscription.provider {
            ProviderEventSourceConfig::Drive => {
                has_drive = true;
                if subscription.subscription_id != WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID {
                    return Err(WebsiteProviderEventIngressError::Config);
                }
                let node_uuid = subscription
                    .drive_node_uuid
                    .as_deref()
                    .ok_or(WebsiteProviderEventIngressError::Config)?;
                validate_bounded_identity(node_uuid, 128)?;
                if node_uuid != expected_node_uuid || !drive_nodes.insert(node_uuid.to_owned()) {
                    return Err(WebsiteProviderEventIngressError::Config);
                }
            }
            ProviderEventSourceConfig::Knowledgebase => {
                has_knowledgebase = true;
                if subscription.drive_node_uuid.is_some() || subscription.organization_id.is_none()
                {
                    return Err(WebsiteProviderEventIngressError::Config);
                }
            }
        }
        let entry = ProviderEventSubscription {
            provider: subscription.provider,
            tenant_id: subscription.tenant_id,
            organization_id: subscription.organization_id,
            drive_node_uuid: subscription.drive_node_uuid,
            signing_secret: secret,
        };
        if subscriptions
            .insert(subscription.subscription_id, entry)
            .is_some()
        {
            return Err(WebsiteProviderEventIngressError::Config);
        }
    }
    if (require_drive && !has_drive) || (require_knowledgebase && !has_knowledgebase) {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    Ok(ValidatedIngressConfig {
        bind_address,
        checkpoint_directory: config.checkpoint_directory,
        maximum_checkpoint_streams: config.maximum_checkpoint_streams,
        maximum_clock_skew_seconds: config.maximum_clock_skew_seconds,
        maximum_concurrent_deliveries: config.maximum_concurrent_deliveries,
        subscriptions,
    })
}

fn validate_subscription_id(value: &str) -> Result<(), WebsiteProviderEventIngressError> {
    if value.len() < 8
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    Ok(())
}

fn validate_bounded_identity(
    value: &str,
    maximum: usize,
) -> Result<(), WebsiteProviderEventIngressError> {
    if value.is_empty()
        || value.len() > maximum
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    Ok(())
}

fn read_secret(
    path: &Path,
    provider: ProviderEventSourceConfig,
) -> Result<Zeroizing<Vec<u8>>, WebsiteProviderEventIngressError> {
    if path.as_os_str().is_empty() {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    let metadata = fs::metadata(path).map_err(|_| WebsiteProviderEventIngressError::Config)?;
    let minimum = 32;
    let maximum = match provider {
        ProviderEventSourceConfig::Drive => 1024,
        ProviderEventSourceConfig::Knowledgebase => MAXIMUM_SECRET_BYTES as usize,
    };
    if !metadata.is_file() || metadata.len() < minimum as u64 || metadata.len() > maximum as u64 + 2
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    let mut secret =
        Zeroizing::new(fs::read(path).map_err(|_| WebsiteProviderEventIngressError::Config)?);
    while matches!(secret.last(), Some(b'\r' | b'\n')) {
        secret.pop();
    }
    if secret.len() < minimum
        || secret.len() > maximum
        || std::str::from_utf8(&secret).is_err()
        || secret.iter().any(u8::is_ascii_control)
    {
        return Err(WebsiteProviderEventIngressError::Config);
    }
    Ok(secret)
}

fn provider_event_router(state: Arc<IngressState>) -> Router {
    Router::new()
        .route(
            "/provider-events/{subscription_id}",
            post(receive_unqualified_event),
        )
        .route(
            "/nodes/{node_uuid}/provider-events/{subscription_id}",
            post(receive_node_event),
        )
        .layer(DefaultBodyLimit::max(MAXIMUM_PROVIDER_EVENT_BYTES))
        .with_state(state)
}

async fn receive_unqualified_event(
    State(state): State<Arc<IngressState>>,
    AxumPath(subscription_id): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    receive_event(state, None, subscription_id, headers, body).await
}

async fn receive_node_event(
    State(state): State<Arc<IngressState>>,
    AxumPath((node_uuid, subscription_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    receive_event(state, Some(node_uuid), subscription_id, headers, body).await
}

async fn receive_event(
    state: Arc<IngressState>,
    route_node_uuid: Option<String>,
    subscription_id: String,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Ok(_permit) = state.concurrency.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS;
    };
    let Some(subscription) = state.subscriptions.get(&subscription_id) else {
        return StatusCode::NOT_FOUND;
    };
    if !delivery_route_matches_subscription(subscription, route_node_uuid.as_deref()) {
        return StatusCode::NOT_FOUND;
    }
    if !content_type_is_json(&headers) {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE;
    }
    if authenticate_delivery(
        subscription,
        &headers,
        &body,
        state.maximum_clock_skew_seconds,
    )
    .is_err()
    {
        return StatusCode::UNAUTHORIZED;
    }
    let event = match parse_website_provider_event(&body) {
        Ok(event) => event,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    if !delivery_matches_event(subscription, &headers, &event) {
        return StatusCode::BAD_REQUEST;
    }
    match state.processor.process_event(event).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(WebsiteProviderEventProcessError::Parse(_)) => StatusCode::BAD_REQUEST,
        Err(
            WebsiteProviderEventProcessError::Checkpoint(_)
            | WebsiteProviderEventProcessError::ContractConflict
            | WebsiteProviderEventProcessError::Uncertainty(_)
            | WebsiteProviderEventProcessError::Reconciliation(_)
            | WebsiteProviderEventProcessError::Invalidation(_),
        ) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn delivery_route_matches_subscription(
    subscription: &ProviderEventSubscription,
    route_node_uuid: Option<&str>,
) -> bool {
    match subscription.provider {
        ProviderEventSourceConfig::Drive => {
            route_node_uuid.is_some()
                && route_node_uuid == subscription.drive_node_uuid.as_deref()
                && route_node_uuid
                    .is_some_and(|value| validate_bounded_identity(value, 128).is_ok())
        }
        ProviderEventSourceConfig::Knowledgebase => route_node_uuid.is_none(),
    }
}

fn content_type_is_json(headers: &HeaderMap) -> bool {
    header_value(headers, CONTENT_TYPE.as_str(), 64) == Ok("application/json")
}

fn authenticate_delivery(
    subscription: &ProviderEventSubscription,
    headers: &HeaderMap,
    body: &[u8],
    maximum_clock_skew_seconds: u64,
) -> Result<(), ()> {
    let timestamp_header = match subscription.provider {
        ProviderEventSourceConfig::Drive => DRIVE_EVENT_TIMESTAMP_HEADER,
        ProviderEventSourceConfig::Knowledgebase => KNOWLEDGEBASE_EVENT_TIME_HEADER,
    };
    let timestamp = header_value(headers, timestamp_header, 64)?;
    validate_delivery_time(subscription.provider, timestamp, maximum_clock_skew_seconds)?;
    let signature = header_value(headers, EVENT_SIGNATURE_HEADER, 80)?;
    let prefix = match subscription.provider {
        ProviderEventSourceConfig::Drive => "v1=",
        ProviderEventSourceConfig::Knowledgebase => "sha256=",
    };
    let Some(signature_hex) = signature.strip_prefix(prefix) else {
        return Err(());
    };
    if signature_hex.len() != 64 || !signature_hex.bytes().all(is_lower_hex) {
        return Err(());
    }
    let mut payload = Vec::with_capacity(timestamp.len() + 1 + body.len());
    payload.extend_from_slice(timestamp.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(body);
    let derived_signing_key = match subscription.provider {
        ProviderEventSourceConfig::Drive => {
            let channel_id = header_value(
                headers,
                DRIVE_CHANNEL_ID_HEADER,
                MAXIMUM_DRIVE_CHANNEL_ID_BYTES,
            )?;
            let verification_token = derive_website_event_verification_token(
                channel_id,
                subscription.signing_secret.as_slice(),
            );
            Some(Zeroizing::new(
                sha256_hash(verification_token.as_bytes()).into_bytes(),
            ))
        }
        ProviderEventSourceConfig::Knowledgebase => None,
    };
    let signing_key = derived_signing_key
        .as_ref()
        .map_or(subscription.signing_secret.as_slice(), |key| key.as_slice());
    let expected = format!("{prefix}{}", hmac_sha256(&payload, signing_key));
    if !secure_compare(&expected, signature) {
        return Err(());
    }
    Ok(())
}

fn validate_delivery_time(
    provider: ProviderEventSourceConfig,
    value: &str,
    maximum_clock_skew_seconds: u64,
) -> Result<(), ()> {
    let delivered_at = match provider {
        ProviderEventSourceConfig::Drive => {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(());
            }
            value.parse::<i64>().map_err(|_| ())?
        }
        ProviderEventSourceConfig::Knowledgebase => OffsetDateTime::parse(value, &Rfc3339)
            .map_err(|_| ())?
            .unix_timestamp(),
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if now.abs_diff(delivered_at) > maximum_clock_skew_seconds {
        return Err(());
    }
    Ok(())
}

fn delivery_matches_event(
    subscription: &ProviderEventSubscription,
    headers: &HeaderMap,
    event: &sdkwork_webserver_delivery_runtime::WebsiteProviderEvent,
) -> bool {
    let expected_source = match subscription.provider {
        ProviderEventSourceConfig::Drive => WebsiteProviderEventSource::Drive,
        ProviderEventSourceConfig::Knowledgebase => WebsiteProviderEventSource::Knowledgebase,
    };
    if event.scope.source != expected_source
        || event.scope.tenant_id != subscription.tenant_id
        || event.scope.organization_id != subscription.organization_id
        || header_value(headers, EVENT_ID_HEADER, 128) != Ok(event.id.as_str())
    {
        return false;
    }
    match subscription.provider {
        ProviderEventSourceConfig::Drive => {
            let Ok(channel_id) = header_value(
                headers,
                DRIVE_CHANNEL_ID_HEADER,
                MAXIMUM_DRIVE_CHANNEL_ID_BYTES,
            ) else {
                return false;
            };
            let Some(node_uuid) = subscription.drive_node_uuid.as_deref() else {
                return false;
            };
            if !valid_drive_retry_count(headers)
                || !event.invalidations.iter().any(|invalidation| {
                    derive_website_event_channel_id(node_uuid, &invalidation.provider_resource_uuid)
                        == channel_id
                })
            {
                return false;
            }
            let expected_idempotency_key = format!("{}:{channel_id}", event.id);
            header_value(
                headers,
                DRIVE_IDEMPOTENCY_KEY_HEADER,
                128 + 1 + MAXIMUM_DRIVE_CHANNEL_ID_BYTES,
            ) == Ok(expected_idempotency_key.as_str())
        }
        ProviderEventSourceConfig::Knowledgebase => {
            header_value(headers, EVENT_TYPE_HEADER, 128) == Ok(event.event_type.as_str())
                && header_value(headers, EVENT_SEQUENCE_HEADER, 32)
                    == Ok(event.sequence_no.to_string().as_str())
        }
    }
}

fn valid_drive_retry_count(headers: &HeaderMap) -> bool {
    let Ok(value) = header_value(headers, DRIVE_RETRY_COUNT_HEADER, 1) else {
        return false;
    };
    value
        .parse::<u8>()
        .is_ok_and(|retry_count| retry_count <= MAXIMUM_DRIVE_RETRY_COUNT)
}

fn header_value<'a>(
    headers: &'a HeaderMap,
    name: &str,
    maximum_bytes: usize,
) -> Result<&'a str, ()> {
    let mut values = headers.get_all(name).iter();
    let value = values.next().ok_or(())?;
    if values.next().is_some() || value.as_bytes().len() > maximum_bytes {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    if value.is_empty() || value.trim() != value {
        return Err(());
    }
    Ok(value)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{HeaderValue, Request},
    };
    use sdkwork_webserver_core::website_runtime::{
        WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry,
    };
    use sdkwork_webserver_delivery_runtime::{
        CachelessWebsiteProviderEventInvalidator, FileWebsiteProviderEventCheckpointStore,
        WebsiteDeliveryExecutor, WebsiteProviderEvent, WebsiteProviderEventProcessor,
        WebsiteProviderEventReconciler, WebsiteProviderRegistry,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;

    struct SuccessfulReconciler;

    #[async_trait]
    impl WebsiteProviderEventReconciler for SuccessfulReconciler {
        async fn reconcile(&self, _event: &WebsiteProviderEvent) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn config_is_loopback_tenant_bound_and_provider_specific() {
        let root = tempfile::tempdir().unwrap();
        let secret = root.path().join("secret");
        fs::write(&secret, "test-only-provider-event-secret-32-bytes").unwrap();
        let config = ProviderEventIngressConfig {
            schema_version: INGRESS_SCHEMA_VERSION.to_owned(),
            bind_address: "127.0.0.1:3810".to_owned(),
            checkpoint_directory: root.path().join("checkpoints"),
            maximum_checkpoint_streams: 8,
            maximum_clock_skew_seconds: 300,
            maximum_concurrent_deliveries: 4,
            subscriptions: vec![ProviderEventSubscriptionConfig {
                subscription_id: "knowledgebase-main".to_owned(),
                provider: ProviderEventSourceConfig::Knowledgebase,
                tenant_scope_hash: "a".repeat(64),
                tenant_id: "100001".to_owned(),
                organization_id: Some("0".to_owned()),
                drive_node_uuid: None,
                secret_file: secret.clone(),
            }],
        };
        assert!(validate_config(config, &"a".repeat(64), "node-1", false, true).is_ok());

        let invalid_drive_subscription = ProviderEventIngressConfig {
            schema_version: INGRESS_SCHEMA_VERSION.to_owned(),
            bind_address: "127.0.0.1:3810".to_owned(),
            checkpoint_directory: root.path().join("drive-checkpoints"),
            maximum_checkpoint_streams: 8,
            maximum_clock_skew_seconds: 300,
            maximum_concurrent_deliveries: 4,
            subscriptions: vec![ProviderEventSubscriptionConfig {
                subscription_id: "drive-website-node-1".to_owned(),
                provider: ProviderEventSourceConfig::Drive,
                tenant_scope_hash: "a".repeat(64),
                tenant_id: "100001".to_owned(),
                organization_id: None,
                drive_node_uuid: Some("node-1".to_owned()),
                secret_file: secret.clone(),
            }],
        };
        assert!(validate_config(
            invalid_drive_subscription,
            &"a".repeat(64),
            "node-1",
            true,
            false,
        )
        .is_err());

        let invalid = ProviderEventIngressConfig {
            schema_version: INGRESS_SCHEMA_VERSION.to_owned(),
            bind_address: "0.0.0.0:3810".to_owned(),
            checkpoint_directory: root.path().join("checkpoints-2"),
            maximum_checkpoint_streams: 8,
            maximum_clock_skew_seconds: 300,
            maximum_concurrent_deliveries: 4,
            subscriptions: Vec::new(),
        };
        assert!(validate_config(invalid, &"a".repeat(64), "node-1", false, false).is_err());
    }

    #[test]
    fn checked_in_provider_event_config_example_matches_its_schema() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../specs/sdkwork.website-provider-event-ingress.schema.json"
        ))
        .unwrap();
        let example: Value = serde_json::from_str(include_str!(
            "../../../etc/data-plane/website-provider-events.development.json.example"
        ))
        .unwrap();
        let validator = jsonschema::draft202012::new(&schema).unwrap();
        let errors = validator
            .iter_errors(&example)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "schema violations: {errors:?}");
    }

    #[tokio::test]
    async fn signed_knowledgebase_delivery_is_accepted_and_tampering_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let executor = Arc::new(WebsiteDeliveryExecutor::new(
            Arc::new(WebsiteRuntimeRegistry::new(
                "node-1",
                WebsiteRuntimeEnvironment::Development,
            )),
            Arc::new(WebsiteProviderRegistry::new()),
        ));
        let checkpoints = Arc::new(
            FileWebsiteProviderEventCheckpointStore::open(root.path().join("checkpoints"), 8)
                .unwrap(),
        );
        let processor = Arc::new(WebsiteProviderEventProcessor::new(
            checkpoints,
            executor.provider_event_invalidator(),
            Arc::new(SuccessfulReconciler),
        ));
        let secret = b"test-only-provider-event-secret-32-bytes";
        let state = Arc::new(IngressState {
            subscriptions: BTreeMap::from([(
                "knowledgebase-main".to_owned(),
                ProviderEventSubscription {
                    provider: ProviderEventSourceConfig::Knowledgebase,
                    tenant_id: "100001".to_owned(),
                    organization_id: Some("0".to_owned()),
                    drive_node_uuid: None,
                    signing_secret: Zeroizing::new(secret.to_vec()),
                },
            )]),
            processor,
            maximum_clock_skew_seconds: 300,
            concurrency: Arc::new(Semaphore::new(4)),
        });
        let app = provider_event_router(state);
        let body = serde_json::to_vec(&wiki_event()).unwrap();
        let request = signed_knowledgebase_request(&body, secret, "knowledgebase-main");
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
        let invalidations = executor
            .provider_resolution_cache_snapshot()
            .await
            .invalidations;
        assert!(
            invalidations > 0,
            "the accepted delivery must invalidate the executor-owned cache"
        );
        let request = signed_knowledgebase_request(&body, secret, "knowledgebase-main");
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            executor
                .provider_resolution_cache_snapshot()
                .await
                .invalidations,
            invalidations,
            "a replayed delivery must not invalidate the cache twice"
        );

        let mut request = signed_knowledgebase_request(&body, secret, "knowledgebase-main");
        request.headers_mut().insert(
            EVENT_SIGNATURE_HEADER,
            HeaderValue::from_static(
                "sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn signed_drive_generation_delivery_is_accepted_and_metadata_tampering_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let checkpoints = Arc::new(
            FileWebsiteProviderEventCheckpointStore::open(root.path().join("checkpoints"), 8)
                .unwrap(),
        );
        let processor = Arc::new(WebsiteProviderEventProcessor::new(
            checkpoints,
            Arc::new(CachelessWebsiteProviderEventInvalidator),
            Arc::new(SuccessfulReconciler),
        ));
        let node_derivation_secret = b"drive-test-node-derivation-secret-at-least-32-bytes";
        let state = Arc::new(IngressState {
            subscriptions: BTreeMap::from([(
                WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID.to_owned(),
                ProviderEventSubscription {
                    provider: ProviderEventSourceConfig::Drive,
                    tenant_id: "tenant-1".to_owned(),
                    organization_id: None,
                    drive_node_uuid: Some("node-1".to_owned()),
                    signing_secret: Zeroizing::new(node_derivation_secret.to_vec()),
                },
            )]),
            processor,
            maximum_clock_skew_seconds: 300,
            concurrency: Arc::new(Semaphore::new(4)),
        });
        let app = provider_event_router(state);
        let body = serde_json::to_vec(&drive_generation_event()).unwrap();
        let request = signed_drive_request(&body, node_derivation_secret);
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );

        let mut request = signed_drive_request(&body, node_derivation_secret);
        *request.uri_mut() = format!("/provider-events/{WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID}")
            .parse()
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );

        let mut request = signed_drive_request(&body, node_derivation_secret);
        *request.uri_mut() =
            format!("/nodes/node-2/provider-events/{WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID}")
                .parse()
                .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );

        let mut request = signed_drive_request(&body, node_derivation_secret);
        *request.uri_mut() = "/nodes/node-1/provider-events/drive-website-node-1"
            .parse()
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
        let request = signed_drive_request(&body, node_derivation_secret);
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );

        let mut request = signed_drive_request(&body, node_derivation_secret);
        request.headers_mut().insert(
            DRIVE_IDEMPOTENCY_KEY_HEADER,
            HeaderValue::from_static("drive-event-1:another-channel"),
        );
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );

        let mut request = signed_drive_request(&body, node_derivation_secret);
        request
            .headers_mut()
            .insert(DRIVE_RETRY_COUNT_HEADER, HeaderValue::from_static("10"));
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    fn signed_drive_request(body: &[u8], node_derivation_secret: &[u8]) -> Request<Body> {
        let timestamp = OffsetDateTime::now_utc().unix_timestamp().to_string();
        let mut payload = Vec::with_capacity(timestamp.len() + 1 + body.len());
        payload.extend_from_slice(timestamp.as_bytes());
        payload.push(b'.');
        payload.extend_from_slice(body);
        let channel_id =
            derive_website_event_channel_id("node-1", "6ecf7e32-4f07-4c78-b6b8-a8b5dd0af02a");
        let verification_token =
            derive_website_event_verification_token(&channel_id, node_derivation_secret);
        let signing_key = sha256_hash(verification_token.as_bytes());
        Request::post(format!(
            "/nodes/node-1/provider-events/{WEBSITE_PROVIDER_EVENT_SUBSCRIPTION_ID}"
        ))
        .header(CONTENT_TYPE, "application/json")
        .header(EVENT_ID_HEADER, "drive-event-1")
        .header(DRIVE_EVENT_TIMESTAMP_HEADER, &timestamp)
        .header(
            EVENT_SIGNATURE_HEADER,
            format!("v1={}", hmac_sha256(&payload, signing_key.as_bytes())),
        )
        .header(DRIVE_CHANNEL_ID_HEADER, &channel_id)
        .header(DRIVE_RETRY_COUNT_HEADER, "0")
        .header(
            DRIVE_IDEMPOTENCY_KEY_HEADER,
            format!("drive-event-1:{channel_id}"),
        )
        .body(Body::from(body.to_vec()))
        .unwrap()
    }

    fn signed_knowledgebase_request(
        body: &[u8],
        secret: &[u8],
        subscription: &str,
    ) -> Request<Body> {
        let timestamp = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let mut payload = Vec::with_capacity(timestamp.len() + 1 + body.len());
        payload.extend_from_slice(timestamp.as_bytes());
        payload.push(b'.');
        payload.extend_from_slice(body);
        Request::post(format!("/provider-events/{subscription}"))
            .header(CONTENT_TYPE, "application/json")
            .header(EVENT_ID_HEADER, "b9cb15ba-f69a-4ab5-a34f-a80ba9348681")
            .header(EVENT_SEQUENCE_HEADER, "42")
            .header(EVENT_TYPE_HEADER, "knowledgebase.wiki.route.revoked.v1")
            .header(KNOWLEDGEBASE_EVENT_TIME_HEADER, &timestamp)
            .header(
                EVENT_SIGNATURE_HEADER,
                format!("sha256={}", hmac_sha256(&payload, secret)),
            )
            .body(Body::from(body.to_vec()))
            .unwrap()
    }

    fn wiki_event() -> Value {
        json!({
            "id": "b9cb15ba-f69a-4ab5-a34f-a80ba9348681",
            "type": "knowledgebase.wiki.route.revoked.v1",
            "source": "sdkwork-knowledgebase",
            "specversion": "1.0",
            "time": "2026-07-22T00:00:00Z",
            "tenantId": "100001",
            "organizationId": "0",
            "subject": "wiki-publication:2ca86ece-5057-459c-99b6-e57d889efea0",
            "sequenceNo": "42",
            "data": {
                "providerResourceUuid": "2ca86ece-5057-459c-99b6-e57d889efea0",
                "providerGeneration": "3",
                "navigationGeneration": "4",
                "searchGeneration": "5",
                "route": "/docs/index",
                "pagePublicVersion": "7",
                "previousPagePublicVersion": "6",
                "operation": "REVOKE",
                "reason": "source_removed"
            }
        })
    }

    fn drive_generation_event() -> Value {
        json!({
            "id": "drive-event-1",
            "type": "drive.website_root.generation.changed.v1",
            "source": "sdkwork-drive",
            "specversion": "1.0",
            "time": "2026-07-22T00:00:00Z",
            "tenantId": "tenant-1",
            "subject": "drive://spaces/space-1/website_roots/6ecf7e32-4f07-4c78-b6b8-a8b5dd0af02a",
            "actorId": "user-1",
            "sequenceNo": "2",
            "data": {
                "operationId": "sync-1",
                "spaceId": "space-1",
                "websiteRootUuid": "6ecf7e32-4f07-4c78-b6b8-a8b5dd0af02a",
                "previousRootNodeId": "node-generation-1",
                "rootNodeId": "node-generation-2",
                "previousGeneration": "1",
                "generation": "2",
                "manifestSha256": format!("sha256:{}", "a".repeat(64)),
                "fileCount": "2",
                "totalBytes": "42",
                "changeReason": "SYNC_ACTIVATED"
            }
        })
    }
}
