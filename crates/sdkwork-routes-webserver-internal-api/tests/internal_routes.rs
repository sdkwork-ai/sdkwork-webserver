use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Extension,
};
use http_body_util::BodyExt;
use sdkwork_routes_webserver_internal_api::build_router_with_internal_api;
use sdkwork_webserver_contract::{
    ClusterDrainCompleteResponse, ClusterHeartbeatRequest, ClusterHeartbeatResponse,
    ClusterHostRef, ClusterInstanceRef, ClusterPeerDirectoryResponse, ClusterRef,
    ClusterRegistrationRequest, ClusterRegistrationResponse, ClusterSyncAckRequest,
    ClusterSyncManifest, ClusterSyncState, CreateRuntimeObservationRequest,
    PublishRuntimeAssignmentRequest, RuntimeAssignment, RuntimeAssignmentDelivery,
    RuntimeObservation, WebInternalApi, WebInternalRequestContext, WebServiceError,
    WebServiceResult,
};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone)]
struct TestInternalApi;

#[async_trait]
impl WebInternalApi for TestInternalApi {
    async fn publish_runtime_assignment(
        &self,
        _context: &WebInternalRequestContext,
        node_uuid: &str,
        environment: &str,
        request: &PublishRuntimeAssignmentRequest,
    ) -> WebServiceResult<RuntimeAssignment> {
        Ok(RuntimeAssignment {
            assignment_uuid: "019b0000-0000-7000-8000-000000000001".to_owned(),
            node_uuid: node_uuid.to_owned(),
            environment: environment.to_owned(),
            generation: request.runtime_set.generation.to_string(),
            snapshot_uuid: request.runtime_set.snapshot_uuid.clone(),
            snapshot_sha256: request.runtime_set.snapshot_sha256.clone(),
            assigned_at: "2026-07-22T00:00:00Z".to_owned(),
        })
    }

    async fn retrieve_current_runtime_assignment(
        &self,
        context: &WebInternalRequestContext,
        environment: &str,
        _if_generation: Option<&str>,
        _if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery> {
        Ok(RuntimeAssignmentDelivery {
            unchanged: true,
            assignment: RuntimeAssignment {
                assignment_uuid: "019b0000-0000-7000-8000-000000000001".to_owned(),
                node_uuid: context
                    .agent_node_uuid
                    .clone()
                    .unwrap_or_else(|| "node-missing".to_owned()),
                environment: environment.to_owned(),
                generation: "7".to_owned(),
                snapshot_uuid: "snapshot-7".to_owned(),
                snapshot_sha256: "a".repeat(64),
                assigned_at: "2026-07-22T00:00:00Z".to_owned(),
            },
            latest_observation_state: None,
            runtime_set: None,
        })
    }

    async fn create_runtime_observation(
        &self,
        context: &WebInternalRequestContext,
        snapshot_uuid: &str,
        request: &CreateRuntimeObservationRequest,
    ) -> WebServiceResult<RuntimeObservation> {
        Ok(RuntimeObservation {
            observation_uuid: "019b0000-0000-7000-8000-000000000002".to_owned(),
            assignment_uuid: "019b0000-0000-7000-8000-000000000001".to_owned(),
            tenant_id: context.tenant_id.to_string(),
            node_uuid: context
                .agent_node_uuid
                .clone()
                .unwrap_or_else(|| "node-missing".to_owned()),
            environment: "production".to_owned(),
            generation: request.generation.clone(),
            snapshot_uuid: snapshot_uuid.to_owned(),
            snapshot_sha256: request.snapshot_sha256.clone(),
            state: request.state,
            node_version: request.node_version.clone(),
            reason_code: request.reason_code.clone(),
            detail: request.detail.clone(),
            observed_at: "2026-07-22T00:00:01Z".to_owned(),
        })
    }

    async fn retrieve_latest_runtime_observation(
        &self,
        _context: &WebInternalRequestContext,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation> {
        Ok(RuntimeObservation {
            observation_uuid: "019b0000-0000-7000-8000-000000000002".to_owned(),
            assignment_uuid: "019b0000-0000-7000-8000-000000000001".to_owned(),
            tenant_id: "42".to_owned(),
            node_uuid: "node-7".to_owned(),
            environment: "production".to_owned(),
            generation: "7".to_owned(),
            snapshot_uuid: snapshot_uuid.to_owned(),
            snapshot_sha256: "a".repeat(64),
            state: sdkwork_webserver_contract::RuntimeObservationState::Active,
            node_version: Some("1.0.0".to_owned()),
            reason_code: None,
            detail: None,
            observed_at: "2026-07-22T00:00:04Z".to_owned(),
        })
    }

    // Self-reporting cluster plane (added by 2e711dac). The stub only needs to
    // satisfy the trait so the runtime-assignment routes can be exercised.
    async fn register_cluster_instance(
        &self,
        context: &WebInternalRequestContext,
        _request: &ClusterRegistrationRequest,
    ) -> WebServiceResult<ClusterRegistrationResponse> {
        Ok(ClusterRegistrationResponse {
            cluster: ClusterRef {
                id: "cluster-1".to_owned(),
                name: "cluster".to_owned(),
                code: "default".to_owned(),
            },
            host: ClusterHostRef {
                id: "host-1".to_owned(),
                name: "host".to_owned(),
                hostname: "localhost".to_owned(),
            },
            instance: ClusterInstanceRef {
                id: context
                    .agent_node_uuid
                    .clone()
                    .unwrap_or_else(|| "instance-1".to_owned()),
                name: "instance".to_owned(),
                role: "edge".to_owned(),
            },
            instance_token: "winst_test".to_owned(),
            heartbeat_interval_seconds: 30,
            offline_threshold_seconds: 90,
            peers: Vec::new(),
        })
    }

    async fn record_cluster_heartbeat(
        &self,
        context: &WebInternalRequestContext,
        _request: &ClusterHeartbeatRequest,
    ) -> WebServiceResult<ClusterHeartbeatResponse> {
        Ok(ClusterHeartbeatResponse {
            instance_id: context
                .agent_node_uuid
                .clone()
                .unwrap_or_else(|| "instance-1".to_owned()),
            status: 1,
            acknowledged_at: "2026-07-22T00:00:05Z".to_owned(),
            heartbeat_interval_seconds: 30,
            offline_threshold_seconds: 90,
            peers: Vec::new(),
            messages: Vec::new(),
            sync: None,
            ops: None,
        })
    }

    async fn retrieve_cluster_sync_manifest(
        &self,
        _context: &WebInternalRequestContext,
        _kind: &str,
    ) -> WebServiceResult<ClusterSyncManifest> {
        Err(WebServiceError::not_found("no sync revision"))
    }

    async fn record_cluster_sync_ack(
        &self,
        _context: &WebInternalRequestContext,
        request: &ClusterSyncAckRequest,
    ) -> WebServiceResult<ClusterSyncState> {
        Ok(ClusterSyncState {
            kind: request.kind.clone(),
            desired_revision: Some(request.revision.clone()),
            applied_revision: Some(request.revision.clone()),
            status: request.status.clone(),
            updated_at: None,
        })
    }

    async fn record_cluster_drain_complete(
        &self,
        _context: &WebInternalRequestContext,
    ) -> WebServiceResult<ClusterDrainCompleteResponse> {
        Ok(ClusterDrainCompleteResponse {
            acknowledged_at: "2026-07-22T00:00:05Z".to_owned(),
        })
    }

    async fn retrieve_cluster_peers(
        &self,
        context: &WebInternalRequestContext,
    ) -> WebServiceResult<ClusterPeerDirectoryResponse> {
        Ok(ClusterPeerDirectoryResponse {
            instance_id: context
                .agent_node_uuid
                .clone()
                .unwrap_or_else(|| "instance-1".to_owned()),
            peers: Vec::new(),
        })
    }
}

#[tokio::test]
async fn canonical_internal_routes_return_sdkwork_resource_envelopes() {
    let app = build_router_with_internal_api(TestInternalApi).layer(Extension(agent_context()));
    let publish = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/internal/v3/api/web/runtime_assignments/node-7/production",
            json!({"runtimeSet": runtime_set()}),
        ))
        .await
        .unwrap();
    assert_eq!(publish.status(), StatusCode::OK);
    let publish_body = response_json(publish).await;
    assert_eq!(
        publish_body.pointer("/data/item/nodeUuid"),
        Some(&json!("node-7"))
    );

    let current = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/internal/v3/api/web/runtime_assignments/current?environment=production&ifGeneration=7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current.status(), StatusCode::OK);
    let current_body = response_json(current).await;
    assert_eq!(
        current_body.pointer("/data/item/unchanged"),
        Some(&json!(true))
    );

    let observation = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/internal/v3/api/web/runtime_assignments/snapshot-7/observations",
            json!({
                "generation": "7",
                "snapshotSha256": "a".repeat(64),
                "state": "RECEIVED",
                "nodeVersion": "1.0.0"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(observation.status(), StatusCode::CREATED);
    let observation_body = response_json(observation).await;
    assert_eq!(
        observation_body.pointer("/data/item/state"),
        Some(&json!("RECEIVED"))
    );

    let latest = app
        .oneshot(
            Request::builder()
                .uri("/internal/v3/api/web/runtime_assignments/snapshot-7/observations/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(latest.status(), StatusCode::OK);
    let latest_body = response_json(latest).await;
    assert_eq!(
        latest_body.pointer("/data/item/environment"),
        Some(&json!("production"))
    );
    assert_eq!(
        latest_body.pointer("/data/item/tenantId"),
        Some(&json!("42"))
    );
    assert_eq!(
        latest_body.pointer("/data/item/state"),
        Some(&json!("ACTIVE"))
    );
}

#[tokio::test]
async fn missing_internal_domain_context_fails_closed() {
    let response = build_router_with_internal_api(TestInternalApi)
        .oneshot(
            Request::builder()
                .uri("/internal/v3/api/web/runtime_assignments/current?environment=production")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
}

fn agent_context() -> WebInternalRequestContext {
    WebInternalRequestContext {
        tenant_id: 42,
        subject_id: "node-7".to_owned(),
        agent_node_uuid: Some("node-7".to_owned()),
        can_publish_cross_tenant: false,
    }
}

fn runtime_set() -> Value {
    json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": "snapshot-7",
        "nodeUuid": "node-7",
        "environment": "production",
        "generation": 7,
        "generatedAt": "2026-07-22T00:00:00Z",
        "compilerVersion": "internal-route-test/1",
        "snapshotSha256": "a".repeat(64),
        "maximumSites": 8,
        "descriptors": []
    })
}

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}
