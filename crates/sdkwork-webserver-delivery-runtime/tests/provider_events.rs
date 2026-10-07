use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use async_trait::async_trait;
use sdkwork_webserver_core::website_runtime::WebsiteProviderType;
use sdkwork_webserver_delivery_runtime::{
    parse_website_provider_event, FileWebsiteProviderEventCheckpointStore, WebsiteProviderEvent,
    WebsiteProviderEventCheckpointStore, WebsiteProviderEventInvalidation,
    WebsiteProviderEventInvalidationKind, WebsiteProviderEventInvalidationPriority,
    WebsiteProviderEventInvalidator, WebsiteProviderEventOrdering,
    WebsiteProviderEventProcessError, WebsiteProviderEventProcessOutcome,
    WebsiteProviderEventProcessor, WebsiteProviderEventReconciler, WebsiteProviderEventScope,
    WebsiteProviderEventSource,
};
use serde_json::{json, Value};

#[derive(Default)]
struct RecordingInvalidator {
    uncertain: AtomicUsize,
    invalidations: Mutex<Vec<Vec<WebsiteProviderEventInvalidation>>>,
    fail: AtomicBool,
}

#[async_trait]
impl WebsiteProviderEventInvalidator for RecordingInvalidator {
    async fn mark_uncertain(&self, _scope: &WebsiteProviderEventScope) -> Result<(), String> {
        self.uncertain.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn invalidate(
        &self,
        invalidations: &[WebsiteProviderEventInvalidation],
    ) -> Result<(), String> {
        if self.fail.load(Ordering::Relaxed) {
            return Err("invalidation unavailable".to_owned());
        }
        self.invalidations
            .lock()
            .unwrap()
            .push(invalidations.to_vec());
        Ok(())
    }
}

#[derive(Default)]
struct CountingReconciler(AtomicUsize);

#[async_trait]
impl WebsiteProviderEventReconciler for CountingReconciler {
    async fn reconcile(&self, _event: &WebsiteProviderEvent) -> Result<(), String> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

struct ConcurrentReconciler {
    barrier: tokio::sync::Barrier,
    active: AtomicUsize,
    maximum_active: AtomicUsize,
}

impl ConcurrentReconciler {
    fn new(parties: usize) -> Self {
        Self {
            barrier: tokio::sync::Barrier::new(parties),
            active: AtomicUsize::new(0),
            maximum_active: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl WebsiteProviderEventReconciler for ConcurrentReconciler {
    async fn reconcile(&self, _event: &WebsiteProviderEvent) -> Result<(), String> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        self.barrier.wait().await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn owner_events_are_strict_and_scope_drive_invalidations_to_website_roots() {
    let drive = serde_json::to_vec(&json!({
        "id": "drive-event-1",
        "type": "drive.node.version.committed.v1",
        "source": "sdkwork-drive",
        "specversion": "1.0",
        "time": "2026-07-22T00:00:00Z",
        "tenantId": "tenant-1",
        "organizationId": "organization-1",
        "subject": "drive://spaces/space-1/nodes/node-1",
        "actorId": "user-1",
        "sequenceNo": "1",
        "data": {
            "operationId": "upload-1",
            "spaceId": "space-1",
            "nodeId": "node-1",
            "driveUri": "drive://spaces/space-1/nodes/node-1",
            "driveVersionId": "version-1",
            "versionNo": "1",
            "spaceRelativePath": "docs/index.html",
            "contentType": "text/html",
            "contentLength": "100",
            "checksumSha256Hex": format!("sha256:{}", "a".repeat(64)),
            "rootScopes": [
                {"scopeId": "root-1", "scopeKind": "WEBSITE_ROOT", "relativePath": "index.html", "rootGeneration": "2"},
                {"scopeId": "raw-1", "scopeKind": "KNOWLEDGEBASE_RAW", "relativePath": "index.html"}
            ]
        }
    }))
    .unwrap();
    let event = parse_website_provider_event(&drive).unwrap();
    assert_eq!(event.ordering, WebsiteProviderEventOrdering::Contiguous);
    assert_eq!(event.invalidations.len(), 1);
    assert_eq!(
        event.invalidations[0].provider_type,
        WebsiteProviderType::Drive
    );
    assert_eq!(event.invalidations[0].provider_resource_uuid, "root-1");

    let mut drive_with_null = serde_json::from_slice::<serde_json::Value>(&drive).unwrap();
    drive_with_null["organizationId"] = serde_json::Value::Null;
    assert!(parse_website_provider_event(&serde_json::to_vec(&drive_with_null).unwrap()).is_err());
    let mut drive_with_invalid_time = serde_json::from_slice::<serde_json::Value>(&drive).unwrap();
    drive_with_invalid_time["time"] = json!("not-a-date-time");
    assert!(
        parse_website_provider_event(&serde_json::to_vec(&drive_with_invalid_time).unwrap())
            .is_err()
    );

    let mut wiki = wiki_route_event(42, "b9cb15ba-f69a-4ab5-a34f-a80ba9348681");
    wiki["actorId"] = json!("must-not-cross-provider-boundary");
    assert!(parse_website_provider_event(&serde_json::to_vec(&wiki).unwrap()).is_err());
    wiki.as_object_mut().unwrap().remove("actorId");
    let mut wiki_with_invalid_null = wiki.clone();
    wiki_with_invalid_null["data"]["driveCheckpoint"] = serde_json::Value::Null;
    assert!(
        parse_website_provider_event(&serde_json::to_vec(&wiki_with_invalid_null).unwrap())
            .is_err()
    );
    wiki_with_invalid_null = wiki.clone();
    wiki_with_invalid_null["data"]["reason"] = serde_json::Value::Null;
    assert!(
        parse_website_provider_event(&serde_json::to_vec(&wiki_with_invalid_null).unwrap())
            .is_err()
    );
    wiki["data"]["sourceFileUuid"] = serde_json::Value::Null;
    wiki["data"]["previousPagePublicVersion"] = serde_json::Value::Null;
    let event = parse_website_provider_event(&serde_json::to_vec(&wiki).unwrap()).unwrap();
    assert_eq!(event.ordering, WebsiteProviderEventOrdering::Monotonic);
    assert_eq!(
        event.invalidations[0].priority,
        WebsiteProviderEventInvalidationPriority::Revocation
    );
}

#[tokio::test]
async fn processor_reconciles_initial_state_deduplicates_and_accepts_wiki_sequence_jumps() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap());
    let invalidator = Arc::new(RecordingInvalidator::default());
    let reconciler = Arc::new(CountingReconciler::default());
    let processor =
        WebsiteProviderEventProcessor::new(store.clone(), invalidator.clone(), reconciler.clone());
    let first = serde_json::to_vec(&wiki_route_event(
        42,
        "b9cb15ba-f69a-4ab5-a34f-a80ba9348681",
    ))
    .unwrap();
    assert_eq!(
        processor.process(&first).await.unwrap(),
        WebsiteProviderEventProcessOutcome::ReconciledAndApplied
    );
    assert_eq!(
        processor.process(&first).await.unwrap(),
        WebsiteProviderEventProcessOutcome::DuplicateIgnored
    );
    let second = serde_json::to_vec(&wiki_route_event(
        99,
        "b9cb15ba-f69a-4ab5-a34f-a80ba9348682",
    ))
    .unwrap();
    assert_eq!(
        processor.process(&second).await.unwrap(),
        WebsiteProviderEventProcessOutcome::Applied
    );
    assert_eq!(reconciler.0.load(Ordering::Relaxed), 1);
    assert_eq!(invalidator.invalidations.lock().unwrap().len(), 2);
    let checkpoint = store
        .load("knowledgebase:100001:0:2ca86ece-5057-459c-99b6-e57d889efea0")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.last_sequence_no(), 99);
    assert!(!checkpoint.is_uncertain());
}

#[tokio::test]
async fn drive_gap_reconciles_and_invalidation_failure_does_not_advance_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap());
    let invalidator = Arc::new(RecordingInvalidator::default());
    let reconciler = Arc::new(CountingReconciler::default());
    let processor =
        WebsiteProviderEventProcessor::new(store.clone(), invalidator.clone(), reconciler.clone());
    let first = drive_event(1, "event-1");
    assert_eq!(
        processor.process_event(first).await.unwrap(),
        WebsiteProviderEventProcessOutcome::ReconciledAndApplied
    );
    invalidator.fail.store(true, Ordering::Relaxed);
    let error = processor.process_event(drive_event(3, "event-3")).await;
    assert!(matches!(
        error,
        Err(WebsiteProviderEventProcessError::Invalidation(_))
    ));
    let checkpoint = store
        .load("drive:tenant-1:-:space-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.last_sequence_no(), 1);
    assert!(!checkpoint.is_uncertain());
    assert_eq!(reconciler.0.load(Ordering::Relaxed), 2);

    invalidator.fail.store(false, Ordering::Relaxed);
    assert_eq!(
        processor
            .process_event(drive_event(3, "event-3"))
            .await
            .unwrap(),
        WebsiteProviderEventProcessOutcome::ReconciledAndApplied
    );
    assert_eq!(
        store
            .load("drive:tenant-1:-:space-1")
            .await
            .unwrap()
            .unwrap()
            .last_sequence_no(),
        3
    );
}

#[tokio::test]
async fn checkpoint_uses_previous_valid_slot_and_conflicts_persist_uncertainty() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap());
    let processor = WebsiteProviderEventProcessor::new(
        store.clone(),
        Arc::new(RecordingInvalidator::default()),
        Arc::new(CountingReconciler::default()),
    );
    processor
        .process_event(drive_event(1, "event-1"))
        .await
        .unwrap();
    processor
        .process_event(drive_event(2, "event-2"))
        .await
        .unwrap();
    let latest_slot = std::fs::read_dir(root.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.file_name().to_string_lossy().ends_with(".a.json"))
        .unwrap()
        .path();
    std::fs::write(latest_slot, "corrupt").unwrap();
    let recovered = FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap();
    assert_eq!(
        recovered
            .load("drive:tenant-1:-:space-1")
            .await
            .unwrap()
            .unwrap()
            .last_sequence_no(),
        1
    );

    let conflict = drive_event(2, "conflicting-event");
    assert!(matches!(
        processor.process_event(conflict).await,
        Err(WebsiteProviderEventProcessError::ContractConflict)
    ));
    assert!(store
        .load("drive:tenant-1:-:space-1")
        .await
        .unwrap()
        .unwrap()
        .is_uncertain());
}

#[tokio::test]
async fn different_event_streams_reconcile_concurrently() {
    let root = tempfile::tempdir().unwrap();
    let reconciler = Arc::new(ConcurrentReconciler::new(2));
    let processor = Arc::new(WebsiteProviderEventProcessor::new(
        Arc::new(FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap()),
        Arc::new(RecordingInvalidator::default()),
        reconciler.clone(),
    ));
    let first = drive_event(1, "event-1");
    let mut second = drive_event(1, "event-2");
    second.scope.tenant_id = "tenant-2".to_owned();
    second.scope.stream_id = "drive:tenant-2:-:space-2".to_owned();

    tokio::time::timeout(Duration::from_secs(2), async {
        let (first, second) = tokio::join!(
            processor.process_event(first),
            processor.process_event(second)
        );
        first.unwrap();
        second.unwrap();
    })
    .await
    .expect("independent streams must not share one global processing lock");
    assert_eq!(reconciler.maximum_active.load(Ordering::SeqCst), 2);
}

#[test]
fn website_root_generation_event_invalidates_the_stable_drive_provider() {
    let body = serde_json::to_vec(&json!({
        "id": "generation-event-1",
        "type": "drive.website_root.generation.changed.v1",
        "source": "sdkwork-drive",
        "specversion": "1.0",
        "time": "2026-07-23T00:00:00Z",
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
    }))
    .unwrap();

    let event = parse_website_provider_event(&body).expect("generation event should parse");
    assert_eq!(event.scope.stream_id, "drive:tenant-1:-:space-1");
    assert_eq!(event.invalidations.len(), 1);
    assert_eq!(
        event.invalidations[0].provider_resource_uuid,
        "6ecf7e32-4f07-4c78-b6b8-a8b5dd0af02a"
    );
    assert_eq!(
        event.invalidations[0].kind,
        WebsiteProviderEventInvalidationKind::Provider
    );
    assert_eq!(
        event.invalidations[0].provider_generation.as_deref(),
        Some("2")
    );

    let mut invalid = serde_json::from_slice::<Value>(&body).unwrap();
    invalid["data"]["generation"] = json!("4");
    assert!(parse_website_provider_event(&serde_json::to_vec(&invalid).unwrap()).is_err());

    let mut invalid = serde_json::from_slice::<Value>(&body).unwrap();
    invalid["subject"] = json!("drive://spaces/space-1/website_roots/other");
    assert!(parse_website_provider_event(&serde_json::to_vec(&invalid).unwrap()).is_err());

    let mut invalid = serde_json::from_slice::<Value>(&body).unwrap();
    invalid["data"]["unexpected"] = json!(true);
    assert!(parse_website_provider_event(&serde_json::to_vec(&invalid).unwrap()).is_err());

    let mut invalid = serde_json::from_slice::<Value>(&body).unwrap();
    invalid["data"]["manifestSha256"] = json!("sha256:not-a-digest");
    assert!(parse_website_provider_event(&serde_json::to_vec(&invalid).unwrap()).is_err());
}

fn drive_event(sequence_no: u64, id: &str) -> WebsiteProviderEvent {
    WebsiteProviderEvent {
        id: id.to_owned(),
        event_type: "drive.node.version.committed.v1".to_owned(),
        sequence_no,
        ordering: WebsiteProviderEventOrdering::Contiguous,
        scope: WebsiteProviderEventScope {
            source: WebsiteProviderEventSource::Drive,
            tenant_id: "tenant-1".to_owned(),
            organization_id: None,
            stream_id: "drive:tenant-1:-:space-1".to_owned(),
        },
        invalidations: vec![WebsiteProviderEventInvalidation {
            provider_type: WebsiteProviderType::Drive,
            provider_resource_uuid: "root-1".to_owned(),
            kind: WebsiteProviderEventInvalidationKind::Route {
                path: "index.html".to_owned(),
            },
            priority: WebsiteProviderEventInvalidationPriority::Normal,
            provider_generation: Some(sequence_no.to_string()),
            public_generation: None,
        }],
        payload_sha256: format!("{:064x}", sequence_no),
    }
}

fn wiki_route_event(sequence_no: u64, id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "type": "knowledgebase.wiki.route.revoked.v1",
        "source": "sdkwork-knowledgebase",
        "specversion": "1.0",
        "time": "2026-07-22T00:00:00Z",
        "tenantId": "100001",
        "organizationId": "0",
        "subject": "wiki-publication:2ca86ece-5057-459c-99b6-e57d889efea0",
        "sequenceNo": sequence_no.to_string(),
        "data": {
            "providerResourceUuid": "2ca86ece-5057-459c-99b6-e57d889efea0",
            "providerGeneration": "3",
            "navigationGeneration": "4",
            "searchGeneration": "5",
            "sourceFileUuid": "3ca86ece-5057-459c-99b6-e57d889efea0",
            "route": "/docs/index",
            "pagePublicVersion": "7",
            "previousPagePublicVersion": "6",
            "operation": "REVOKE",
            "driveCheckpoint": "8",
            "reason": "source_removed"
        }
    })
}

#[tokio::test]
async fn checkpoint_store_tolerates_and_cleans_tmp_residue() {
    // A write cancelled between staging and rename leaves `<digest>.<slot>.tmp`
    // behind. The loader must clean it instead of failing store startup.
    let root = tempfile::tempdir().unwrap();
    let residue = root
        .path()
        .join(format!("{}.a.tmp", "0".repeat(64)));
    std::fs::write(&residue, b"partial write").unwrap();

    let store = FileWebsiteProviderEventCheckpointStore::open(root.path(), 8)
        .expect("tmp residue must not brick the checkpoint store");
    assert!(!residue.exists(), "tmp residue is cleaned during load");
    assert!(store.load("drive:tenant-1:-:space-1").await.unwrap().is_none());

    // The same directory keeps reopening cleanly afterwards.
    FileWebsiteProviderEventCheckpointStore::open(root.path(), 8).unwrap();
}
