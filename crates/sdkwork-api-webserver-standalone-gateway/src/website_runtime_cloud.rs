use sdkwork_utils_rust::uuid;
use sdkwork_web_internal_sdk::{
    api::RuntimeApi, CreateRuntimeObservationRequest, RuntimeAssignment as SdkRuntimeAssignment,
    RuntimeAssignmentDelivery as SdkRuntimeAssignmentDelivery, SdkworkConfig, SdkworkCustomClient,
};
use sdkwork_webserver_contract::RuntimeObservationState;
use sdkwork_webserver_core::website_runtime::MAX_WEBSITE_RUNTIME_SET_BYTES;
use thiserror::Error;

const RESPONSE_ENVELOPE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CloudRuntimeAssignment {
    pub(crate) node_uuid: String,
    pub(crate) environment: String,
    pub(crate) generation: String,
    pub(crate) snapshot_uuid: String,
    pub(crate) snapshot_sha256: String,
}

pub(crate) struct CloudRuntimeDelivery {
    pub(crate) unchanged: bool,
    pub(crate) assignment: CloudRuntimeAssignment,
    pub(crate) latest_observation_state: Option<RuntimeObservationState>,
    pub(crate) runtime_set_bytes: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct CloudRuntimeAssignmentSource {
    api: RuntimeApi,
    node_uuid: String,
    environment: String,
    node_version: String,
}

#[derive(Debug, Error)]
pub(crate) enum CloudRuntimeAssignmentError {
    #[error("Web Internal SDK client construction failed")]
    Client,
    #[error("Web runtime assignment request failed")]
    Request,
    #[error("Web runtime assignment response violated its contract")]
    Response,
}

impl CloudRuntimeAssignmentSource {
    pub(crate) fn new(
        base_url: String,
        node_token: String,
        node_uuid: String,
        environment: String,
        node_version: String,
    ) -> Result<Self, CloudRuntimeAssignmentError> {
        let mut config = SdkworkConfig::new(base_url);
        config.timeout_ms = 30_000;
        config.max_response_body_bytes = MAX_WEBSITE_RUNTIME_SET_BYTES + RESPONSE_ENVELOPE_BYTES;
        let client =
            SdkworkCustomClient::new(config).map_err(|_| CloudRuntimeAssignmentError::Client)?;
        client.set_api_key(node_token);
        Ok(Self {
            api: client.runtime(),
            node_uuid,
            environment,
            node_version,
        })
    }

    pub(crate) fn node_uuid(&self) -> &str {
        &self.node_uuid
    }

    pub(crate) fn environment(&self) -> &str {
        &self.environment
    }

    pub(crate) async fn pull(
        &self,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> Result<CloudRuntimeDelivery, CloudRuntimeAssignmentError> {
        let delivery = self
            .api
            .assignments_current_retrieve(&self.environment, if_generation, if_snapshot_sha256)
            .await
            .map_err(|_| CloudRuntimeAssignmentError::Request)?;
        self.validate_delivery(delivery)
    }

    pub(crate) async fn observe(
        &self,
        assignment: &CloudRuntimeAssignment,
        state: RuntimeObservationState,
        reason_code: Option<&str>,
        detail: Option<&str>,
    ) -> Result<(), CloudRuntimeAssignmentError> {
        let idempotency_key = uuid();
        let observation = self
            .api
            .assignments_observations_create(
                &assignment.snapshot_uuid,
                &CreateRuntimeObservationRequest {
                    generation: assignment.generation.clone(),
                    snapshot_sha256: assignment.snapshot_sha256.clone(),
                    state: state.as_str().to_owned(),
                    node_version: Some(self.node_version.clone()),
                    reason_code: reason_code.map(str::to_owned),
                    detail: detail.map(str::to_owned),
                },
                &idempotency_key,
            )
            .await
            .map_err(|_| CloudRuntimeAssignmentError::Request)?;
        if observation.node_uuid != assignment.node_uuid
            || observation.generation != assignment.generation
            || observation.snapshot_uuid != assignment.snapshot_uuid
            || observation.snapshot_sha256 != assignment.snapshot_sha256
            || observation.state != state.as_str()
        {
            return Err(CloudRuntimeAssignmentError::Response);
        }
        Ok(())
    }

    fn validate_delivery(
        &self,
        delivery: SdkRuntimeAssignmentDelivery,
    ) -> Result<CloudRuntimeDelivery, CloudRuntimeAssignmentError> {
        let assignment =
            validate_assignment(delivery.assignment, &self.node_uuid, &self.environment)?;
        let latest_observation_state = delivery
            .latest_observation_state
            .as_deref()
            .map(RuntimeObservationState::try_from)
            .transpose()
            .map_err(|_| CloudRuntimeAssignmentError::Response)?;
        let runtime_set_bytes = delivery
            .runtime_set
            .map(|runtime_set| serde_json::to_vec(&runtime_set))
            .transpose()
            .map_err(|_| CloudRuntimeAssignmentError::Response)?;
        if delivery.unchanged != runtime_set_bytes.is_none() {
            return Err(CloudRuntimeAssignmentError::Response);
        }
        Ok(CloudRuntimeDelivery {
            unchanged: delivery.unchanged,
            assignment,
            latest_observation_state,
            runtime_set_bytes,
        })
    }
}

fn validate_assignment(
    assignment: SdkRuntimeAssignment,
    expected_node_uuid: &str,
    expected_environment: &str,
) -> Result<CloudRuntimeAssignment, CloudRuntimeAssignmentError> {
    if assignment.node_uuid != expected_node_uuid
        || assignment.environment != expected_environment
        || assignment.assignment_uuid.is_empty()
        || assignment.snapshot_uuid.is_empty()
        || assignment.snapshot_uuid.len() > 128
        || !is_generation(&assignment.generation)
        || !is_sha256(&assignment.snapshot_sha256)
    {
        return Err(CloudRuntimeAssignmentError::Response);
    }
    Ok(CloudRuntimeAssignment {
        node_uuid: assignment.node_uuid,
        environment: assignment.environment,
        generation: assignment.generation,
        snapshot_uuid: assignment.snapshot_uuid,
        snapshot_sha256: assignment.snapshot_sha256,
    })
}

fn is_generation(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value
            .parse::<u64>()
            .is_ok_and(|value| (1..=9_007_199_254_740_991).contains(&value))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use axum::{
        body::Body,
        extract::{Request, State},
        http::StatusCode,
        response::{IntoResponse, Response},
        routing::any,
        Json, Router,
    };
    use http_body_util::BodyExt;
    use sdkwork_utils_rust::is_uuid;
    use sdkwork_webserver_contract::RuntimeObservationState;
    use serde_json::{json, Value};

    use super::{
        is_generation, is_sha256, CloudRuntimeAssignmentError, CloudRuntimeAssignmentSource,
    };

    const NODE_UUID: &str = "node-7";
    const SNAPSHOT_UUID: &str = "snapshot-7";
    const SNAPSHOT_SHA256: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[derive(Clone)]
    struct LoopbackState {
        requests: Arc<Mutex<Vec<CapturedRequest>>>,
        mismatched_node: bool,
    }

    #[derive(Debug)]
    struct CapturedRequest {
        method: String,
        path: String,
        query: Option<String>,
        api_key: Option<String>,
        idempotency_key: Option<String>,
        content_type: Option<String>,
        body: Value,
    }

    #[test]
    fn assignment_scalar_contract_is_strict() {
        assert!(is_generation("1"));
        assert!(is_generation("9007199254740991"));
        for invalid in ["", "0", "01", "9007199254740992"] {
            assert!(!is_generation(invalid));
        }
        assert!(is_sha256(&"a".repeat(64)));
        assert!(!is_sha256(&"A".repeat(64)));
    }

    #[tokio::test]
    async fn generated_internal_sdk_preserves_runtime_assignment_wire_contract() {
        let (base_url, requests, server) = spawn_loopback(false).await;
        let source = CloudRuntimeAssignmentSource::new(
            base_url,
            "node-token-7".to_owned(),
            NODE_UUID.to_owned(),
            "production".to_owned(),
            "1.2.3".to_owned(),
        )
        .unwrap();

        let delivery = source.pull(Some("6"), Some(&"b".repeat(64))).await.unwrap();
        assert!(delivery.unchanged);
        assert!(delivery.runtime_set_bytes.is_none());
        assert_eq!(
            delivery.latest_observation_state,
            Some(RuntimeObservationState::Staged)
        );

        source
            .observe(
                &delivery.assignment,
                RuntimeObservationState::Active,
                None,
                None,
            )
            .await
            .unwrap();

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let pull = &requests[0];
        assert_eq!(pull.method, "GET");
        assert_eq!(
            pull.path,
            "/internal/v3/api/web/runtime_assignments/current"
        );
        assert_eq!(pull.api_key.as_deref(), Some("node-token-7"));
        assert_eq!(
            query_parameters(pull.query.as_deref().unwrap()),
            BTreeMap::from([
                ("environment".to_owned(), "production".to_owned()),
                ("ifGeneration".to_owned(), "6".to_owned()),
                ("ifSnapshotSha256".to_owned(), "b".repeat(64)),
            ])
        );

        let observation = &requests[1];
        assert_eq!(observation.method, "POST");
        assert_eq!(
            observation.path,
            "/internal/v3/api/web/runtime_assignments/snapshot-7/observations"
        );
        assert_eq!(observation.api_key.as_deref(), Some("node-token-7"));
        assert!(observation.idempotency_key.as_deref().is_some_and(is_uuid));
        assert_eq!(
            observation.content_type.as_deref(),
            Some("application/json")
        );
        assert_eq!(
            observation.body,
            json!({
                "generation": "7",
                "snapshotSha256": SNAPSHOT_SHA256,
                "state": "ACTIVE",
                "nodeVersion": "1.2.3"
            })
        );
        drop(requests);
        server.abort();
    }

    #[tokio::test]
    async fn generated_internal_sdk_response_identity_mismatch_fails_closed() {
        let (base_url, _requests, server) = spawn_loopback(true).await;
        let source = CloudRuntimeAssignmentSource::new(
            base_url,
            "node-token-7".to_owned(),
            NODE_UUID.to_owned(),
            "production".to_owned(),
            "1.2.3".to_owned(),
        )
        .unwrap();

        assert!(matches!(
            source.pull(None, None).await,
            Err(CloudRuntimeAssignmentError::Response)
        ));
        server.abort();
    }

    async fn spawn_loopback(
        mismatched_node: bool,
    ) -> (
        String,
        Arc<Mutex<Vec<CapturedRequest>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .fallback(any(loopback_handler))
            .with_state(LoopbackState {
                requests: Arc::clone(&requests),
                mismatched_node,
            });
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), requests, server)
    }

    async fn loopback_handler(State(state): State<LoopbackState>, request: Request) -> Response {
        let method = request.method().as_str().to_owned();
        let path = request.uri().path().to_owned();
        let query = request.uri().query().map(str::to_owned);
        let api_key = header_value(&request, "x-api-key");
        let idempotency_key = header_value(&request, "idempotency-key");
        let content_type = header_value(&request, "content-type");
        let body_bytes = request.into_body().collect().await.unwrap().to_bytes();
        let body = if body_bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body_bytes).unwrap()
        };
        state.requests.lock().unwrap().push(CapturedRequest {
            method: method.clone(),
            path,
            query,
            api_key,
            idempotency_key,
            content_type,
            body,
        });

        let node_uuid = if state.mismatched_node {
            "node-other"
        } else {
            NODE_UUID
        };
        let (status, item) = if method == "POST" {
            (
                StatusCode::CREATED,
                json!({
                    "observationUuid": "observation-7",
                    "tenantId": "7",
                    "assignmentUuid": "assignment-7",
                    "nodeUuid": node_uuid,
                    "environment": "production",
                    "generation": "7",
                    "snapshotUuid": SNAPSHOT_UUID,
                    "snapshotSha256": SNAPSHOT_SHA256,
                    "state": "ACTIVE",
                    "nodeVersion": "1.2.3",
                    "observedAt": "2026-07-22T00:00:01Z"
                }),
            )
        } else {
            (
                StatusCode::OK,
                json!({
                    "unchanged": true,
                    "assignment": {
                        "assignmentUuid": "assignment-7",
                        "nodeUuid": node_uuid,
                        "environment": "production",
                        "generation": "7",
                        "snapshotUuid": SNAPSHOT_UUID,
                        "snapshotSha256": SNAPSHOT_SHA256,
                        "assignedAt": "2026-07-22T00:00:00Z"
                    },
                    "latestObservationState": "STAGED"
                }),
            )
        };
        (
            status,
            Json(json!({
                "code": 0,
                "message": "success",
                "data": {"item": item},
                "traceId": "trace-7",
                "timestamp": "2026-07-22T00:00:02Z"
            })),
        )
            .into_response()
    }

    fn header_value(request: &Request<Body>, name: &str) -> Option<String> {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    fn query_parameters(query: &str) -> BTreeMap<String, String> {
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }
}
