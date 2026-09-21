//! Distributed cluster operations for the Web Server self-cluster.
//!
//! One cluster groups many hosts; every host runs one or more webserver
//! process instances. Instances register through the machine-only internal
//! surface (shared registration credential `wreg_…`, then their own heartbeat
//! token `winst_…`), heartbeat liveness + health + metrics, and talk to each
//! other through the control-plane peer directory and the peer message
//! mailbox. The admin surface (backend-api, platform operator tenant 0)
//! manages the inventory and reads the monitoring views; a bounded liveness
//! sweep marks silent members offline and writes lifecycle events.

use chrono::{Duration, Utc};
use sdkwork_database_id::uuid_v4;
use sdkwork_utils_rust::crypto::{secure_compare, sha256_hash};
use sdkwork_webserver_contract::{
    cluster_quality_score, web_is_platform_operator_tenant, ClusterEventPage,
    ClusterHeartbeatRequest, ClusterHeartbeatResponse, ClusterHeartbeatSamplePage, ClusterHostPage,
    ClusterHostResponse, ClusterInstancePage, ClusterInstanceResponse, ClusterOverviewResponse,
    ClusterPage, ClusterPeerDirectoryResponse, ClusterQualityMetrics, ClusterRegistrationRequest,
    ClusterRegistrationResponse, ClusterResponse, ClusterSyncAckRequest, ClusterSyncManifest,
    ClusterSyncState, CreateClusterRequest, EnqueueClusterPeerMessagesRequest,
    EnqueueClusterPeerMessagesResponse, UpdateClusterHostRequest, UpdateClusterInstanceRequest,
    UpdateClusterRequest, WebBackendRequestContext, WebServiceError, WebServiceResult,
    CLUSTER_ENVIRONMENTS, CLUSTER_EVENT_SEVERITIES, CLUSTER_HEALTH_STATES, CLUSTER_INSTANCE_ROLES,
    CLUSTER_JOIN_MODES, CLUSTER_JOIN_MODE_TUNNEL, CLUSTER_SYNC_KIND_APPLICATIONS,
    CLUSTER_SYNC_KIND_CONFIG, CLUSTER_SYNC_STATUS_IN_SYNC, CLUSTER_SYNC_STATUS_PENDING,
};

use crate::repository::{
    ClusterEventWrite, ClusterHeartbeatWrite, ClusterHostUpsert, ClusterInstanceCredentials,
    ClusterInstanceUpsert, ClusterPeerMessageEnqueue, ClusterProbeOutcome, ClusterProbeWrite,
    ClusterSyncAckWrite, ClusterSyncRevisionPublish,
};
use crate::WebService;

/// Prefix of per-instance heartbeat tokens issued at registration.
pub const CLUSTER_INSTANCE_TOKEN_PREFIX: &str = "winst_";
/// Prefix of the shared cluster registration credential (`wreg_…`).
pub const CLUSTER_REGISTRATION_TOKEN_PREFIX: &str = "wreg_";
/// Principal subject injected for requests authenticated with the shared
/// registration credential.
pub const CLUSTER_REGISTRATION_SUBJECT: &str = "cluster-registration";
/// App id reported for cluster machine principals.
pub const CLUSTER_MACHINE_APP_ID: &str = "sdkwork-webserver-cluster";

const MAX_PEERS_PER_RESPONSE: i32 = 200;
const MAX_MESSAGES_PER_HEARTBEAT: i32 = 32;
const MAX_METRICS_BYTES: usize = 16 * 1024;
const MAX_LOCAL_IPS: usize = 64;
const MAX_MAC_ADDRESSES: usize = 64;
const MAX_ADDRESS_BYTES: usize = 64;
const SWEEP_BATCH_LIMIT: i32 = 512;
/// Default heartbeat sample retention applied by the liveness sweep when the
/// `SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_RETENTION_HOURS` env is absent.
const DEFAULT_HEARTBEAT_RETENTION_HOURS: i64 = 72;

/// Machine principal produced by cluster credential authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterMachineCredential {
    pub tenant_id: i64,
    pub subject_id: String,
    pub permission_scope: Vec<String>,
}

/// Bounded summary of one liveness sweep tick for logging.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClusterSweepReport {
    pub instances_expired: usize,
    pub hosts_expired: usize,
    pub peer_messages_expired: usize,
    pub heartbeats_purged: usize,
}

impl WebService {
    // ------------------------------------------------------------------
    // Admin surface (platform operator tenant 0)
    // ------------------------------------------------------------------

    pub async fn cluster_list(
        &self,
        context: &WebBackendRequestContext,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ClusterPage> {
        require_cluster_platform_operator(context)?;
        self.repository.list_clusters(page, page_size).await
    }

    pub async fn cluster_create(
        &self,
        context: &WebBackendRequestContext,
        request: &CreateClusterRequest,
    ) -> WebServiceResult<ClusterResponse> {
        require_cluster_platform_operator(context)?;
        validate_cluster_thresholds(
            request.heartbeat_interval_seconds.unwrap_or(15),
            request.offline_threshold_seconds.unwrap_or(60),
        )?;
        let created = self
            .repository
            .create_cluster(request)
            .await
            .map_err(|error| {
                if matches!(error, WebServiceError::Conflict(_)) {
                    WebServiceError::conflict("cluster code already exists")
                } else {
                    error
                }
            })?;
        self.record_cluster_event(
            &created.id,
            None,
            None,
            "CLUSTER_CREATED",
            "INFO",
            &format!("cluster {} ({}) created", created.name, created.code),
        )
        .await;
        Ok(created)
    }

    pub async fn cluster_retrieve(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<ClusterResponse> {
        require_cluster_platform_operator(context)?;
        self.repository.retrieve_cluster(cluster_id).await
    }

    pub async fn cluster_update(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        request: &UpdateClusterRequest,
    ) -> WebServiceResult<ClusterResponse> {
        require_cluster_platform_operator(context)?;
        if request.name.is_none()
            && request.description.is_none()
            && request.status.is_none()
            && request.heartbeat_interval_seconds.is_none()
            && request.offline_threshold_seconds.is_none()
        {
            return Err(WebServiceError::validation(
                "update request contains no fields",
            ));
        }
        if let Some(status) = request.status {
            if !(0..=1).contains(&status) {
                return Err(WebServiceError::validation(
                    "status must be 0 (inactive) or 1 (active)",
                ));
            }
        }
        validate_cluster_thresholds(
            request.heartbeat_interval_seconds.unwrap_or(15),
            request.offline_threshold_seconds.unwrap_or(60),
        )?;
        let updated = self.repository.update_cluster(cluster_id, request).await?;
        self.record_cluster_event(
            &updated.id,
            None,
            None,
            "CLUSTER_UPDATED",
            "INFO",
            &format!("cluster {} updated", updated.code),
        )
        .await;
        Ok(updated)
    }

    pub async fn cluster_delete(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
    ) -> WebServiceResult<()> {
        require_cluster_platform_operator(context)?;
        let existing = self.repository.retrieve_cluster(cluster_id).await?;
        if existing.host_count > 0 {
            return Err(WebServiceError::conflict(
                "cluster still contains hosts; reassign or remove them first",
            ));
        }
        self.repository.delete_cluster(cluster_id).await?;
        self.record_cluster_event(
            &existing.id,
            None,
            None,
            "CLUSTER_DELETED",
            "INFO",
            &format!("cluster {} deleted", existing.code),
        )
        .await;
        Ok(())
    }

    pub async fn cluster_host_list(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        status: Option<i32>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHostPage> {
        require_cluster_platform_operator(context)?;
        if let Some(status) = status {
            if !(0..=4).contains(&status) {
                return Err(WebServiceError::validation(
                    "host status must be between 0 and 4",
                ));
            }
        }
        self.repository
            .list_cluster_hosts(cluster_id, status, page_size, cursor)
            .await
    }

    pub async fn cluster_host_retrieve(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<ClusterHostResponse> {
        require_cluster_platform_operator(context)?;
        self.repository.retrieve_cluster_host(host_id).await
    }

    pub async fn cluster_host_update(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
        request: &UpdateClusterHostRequest,
    ) -> WebServiceResult<ClusterHostResponse> {
        require_cluster_platform_operator(context)?;
        if request.name.is_none() && request.cluster_id.is_none() {
            return Err(WebServiceError::validation(
                "update request contains no fields",
            ));
        }
        let previous = self.repository.retrieve_cluster_host(host_id).await?;
        let updated = self
            .repository
            .update_cluster_host(host_id, request)
            .await?;
        if previous.cluster_id != updated.cluster_id {
            self.record_cluster_event(
                &updated.cluster_id,
                Some(host_id),
                None,
                "HOST_REASSIGNED",
                "INFO",
                &format!(
                    "host {} reassigned from cluster {} to {}",
                    previous.hostname, previous.cluster_id, updated.cluster_id
                ),
            )
            .await;
        }
        Ok(updated)
    }

    pub async fn cluster_host_delete(
        &self,
        context: &WebBackendRequestContext,
        host_id: &str,
    ) -> WebServiceResult<()> {
        require_cluster_platform_operator(context)?;
        let existing = self.repository.retrieve_cluster_host(host_id).await?;
        if existing.instance_count > 0 {
            return Err(WebServiceError::conflict(
                "host still contains instances; remove them first",
            ));
        }
        self.repository.delete_cluster_host(host_id).await?;
        self.record_cluster_event(
            &existing.cluster_id,
            Some(host_id),
            None,
            "HOST_REMOVED",
            "WARNING",
            &format!(
                "host {} removed from the cluster inventory",
                existing.hostname
            ),
        )
        .await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn cluster_instance_list(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        host_id: Option<&str>,
        status: Option<i32>,
        health_state: Option<&str>,
        join_mode: Option<i32>,
        sync_status: Option<i32>,
        labels: Option<&str>,
        search: Option<&str>,
        build_version: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterInstancePage> {
        require_cluster_platform_operator(context)?;
        if let Some(status) = status {
            if !(0..=5).contains(&status) {
                return Err(WebServiceError::validation(
                    "instance status must be between 0 and 5",
                ));
            }
        }
        if let Some(join_mode) = join_mode {
            if !(0..=1).contains(&join_mode) {
                return Err(WebServiceError::validation(
                    "joinMode must be between 0 and 1",
                ));
            }
        }
        if let Some(sync_status) = sync_status {
            if !(0..=3).contains(&sync_status) {
                return Err(WebServiceError::validation(
                    "syncStatus must be between 0 and 3",
                ));
            }
        }
        if let Some(health_state) = health_state {
            if !CLUSTER_HEALTH_STATES.contains(&health_state) {
                return Err(WebServiceError::validation(
                    "healthState is not a supported health state",
                ));
            }
        }
        // Label selector: `k1=v1,k2=v2` — every pair must match.
        let label_pairs: Vec<(String, String)> = labels
            .map(|raw| {
                raw.split(',')
                    .filter(|pair| !pair.trim().is_empty())
                    .map(|pair| {
                        let (key, value) = pair.split_once('=').ok_or_else(|| {
                            WebServiceError::validation(format!(
                                "label selector `{pair}` must be key=value"
                            ))
                        })?;
                        Ok((key.trim().to_owned(), value.trim().to_owned()))
                    })
                    .collect::<WebServiceResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        if let Some(search) = search {
            if search.trim().len() > 100 {
                return Err(WebServiceError::validation(
                    "search must be at most 100 bytes",
                ));
            }
        }
        self.repository
            .list_cluster_instances(
                cluster_id,
                host_id,
                status,
                health_state,
                join_mode,
                sync_status,
                &label_pairs,
                search,
                build_version,
                page_size,
                cursor,
            )
            .await
    }

    pub async fn cluster_instance_retrieve(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        require_cluster_platform_operator(context)?;
        self.repository.retrieve_cluster_instance(instance_id).await
    }

    pub async fn cluster_instance_update(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        request: &UpdateClusterInstanceRequest,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        require_cluster_platform_operator(context)?;
        if request.name.is_none()
            && request.status.is_none()
            && request.public_endpoint.is_none()
            && request.routing_enabled.is_none()
            && request.draining.is_none()
            && request.probe_url.is_none()
            && request.labels.is_none()
            && request.routing_weight.is_none()
            && request.maintenance_note.is_none()
        {
            return Err(WebServiceError::validation(
                "update request contains no fields",
            ));
        }
        if let Some(status) = request.status {
            if !(0..=5).contains(&status) {
                return Err(WebServiceError::validation(
                    "instance status must be between 0 and 5",
                ));
            }
        }
        if let Some(weight) = request.routing_weight {
            if !(1..=10_000).contains(&weight) {
                return Err(WebServiceError::validation(
                    "routingWeight must be between 1 and 10000",
                ));
            }
        }
        if let Some(note) = request.maintenance_note.as_deref() {
            if note.len() > 255 {
                return Err(WebServiceError::validation(
                    "maintenanceNote must be at most 255 bytes",
                ));
            }
        }
        let updated = self
            .repository
            .update_cluster_instance(instance_id, request)
            .await?;
        let event_type = if request.status == Some(5) {
            "INSTANCE_MAINTENANCE"
        } else {
            "INSTANCE_UPDATED"
        };
        self.record_cluster_event(
            &updated.cluster_id,
            Some(&updated.host_id),
            Some(instance_id),
            event_type,
            "INFO",
            &format!("instance {} updated by an operator", updated.name),
        )
        .await;
        Ok(updated)
    }

    /// Graceful drain: excludes the instance from cluster routing and asks
    /// the node to finish in-flight work and stop (industry drain, PRD
    /// instance management). Fully reversible via [`Self::cluster_instance_undrain`].
    pub async fn cluster_instance_drain(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let updated = self
            .cluster_instance_update(
                context,
                instance_id,
                &UpdateClusterInstanceRequest {
                    routing_enabled: Some(false),
                    draining: Some(true),
                    ..UpdateClusterInstanceRequest::default()
                },
            )
            .await?;
        self.record_cluster_event(
            &updated.cluster_id,
            Some(&updated.host_id),
            Some(instance_id),
            "INSTANCE_DRAINING",
            "WARNING",
            &format!(
                "instance {} draining: excluded from routing, in-flight work finishing",
                updated.name
            ),
        )
        .await;
        Ok(updated)
    }

    /// Clears drain + cordon: the instance rejoins the routing pool.
    pub async fn cluster_instance_undrain(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let updated = self
            .cluster_instance_update(
                context,
                instance_id,
                &UpdateClusterInstanceRequest {
                    routing_enabled: Some(true),
                    draining: Some(false),
                    ..UpdateClusterInstanceRequest::default()
                },
            )
            .await?;
        self.record_cluster_event(
            &updated.cluster_id,
            Some(&updated.host_id),
            Some(instance_id),
            "INSTANCE_UNDRAINED",
            "INFO",
            &format!("instance {} rejoined the routing pool", updated.name),
        )
        .await;
        Ok(updated)
    }

    /// Cordon: removes the instance from routing without draining (it keeps
    /// serving; new routed requests avoid it).
    pub async fn cluster_instance_cordon(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let updated = self
            .cluster_instance_update(
                context,
                instance_id,
                &UpdateClusterInstanceRequest {
                    routing_enabled: Some(false),
                    ..UpdateClusterInstanceRequest::default()
                },
            )
            .await?;
        self.record_cluster_event(
            &updated.cluster_id,
            Some(&updated.host_id),
            Some(instance_id),
            "INSTANCE_CORDONED",
            "INFO",
            &format!("instance {} cordoned out of routing", updated.name),
        )
        .await;
        Ok(updated)
    }

    /// Uncordon: restores routing participation.
    pub async fn cluster_instance_uncordon(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let updated = self
            .cluster_instance_update(
                context,
                instance_id,
                &UpdateClusterInstanceRequest {
                    routing_enabled: Some(true),
                    ..UpdateClusterInstanceRequest::default()
                },
            )
            .await?;
        self.record_cluster_event(
            &updated.cluster_id,
            Some(&updated.host_id),
            Some(instance_id),
            "INSTANCE_UNCORDONED",
            "INFO",
            &format!("instance {} restored to routing", updated.name),
        )
        .await;
        Ok(updated)
    }

    /// Records one active-probe outcome for an instance (resolved by uuid)
    /// and emits auto-eject / auto-recover lifecycle events.
    pub async fn cluster_instance_probe_outcome(
        &self,
        instance_uuid: &str,
        healthy: bool,
    ) -> WebServiceResult<ClusterProbeOutcome> {
        let credentials: ClusterInstanceCredentials = self
            .repository
            .resolve_cluster_instance_by_uuid(instance_uuid)
            .await?;
        let outcome = self
            .repository
            .record_cluster_probe_outcome(ClusterProbeWrite {
                tenant_id: credentials.tenant_id,
                instance_id: credentials.instance_id,
                healthy,
            })
            .await?;
        if outcome.eject_transition {
            self.record_cluster_event(
                &credentials.cluster_uuid,
                Some(&credentials.host_uuid),
                Some(&credentials.instance_uuid),
                "INSTANCE_AUTO_EJECTED",
                "ERROR",
                &format!(
                    "instance {} auto-ejected from routing after consecutive probe failures",
                    credentials.instance_uuid
                ),
            )
            .await;
        }
        if outcome.recovered {
            self.record_cluster_event(
                &credentials.cluster_uuid,
                Some(&credentials.host_uuid),
                Some(&credentials.instance_uuid),
                "INSTANCE_AUTO_RECOVERED",
                "INFO",
                &format!(
                    "instance {} recovered: probes healthy again, restored to routing",
                    credentials.instance_uuid
                ),
            )
            .await;
        }
        Ok(outcome)
    }

    pub async fn cluster_instance_delete(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
    ) -> WebServiceResult<()> {
        require_cluster_platform_operator(context)?;
        let existing = self
            .repository
            .retrieve_cluster_instance(instance_id)
            .await?;
        self.repository.delete_cluster_instance(instance_id).await?;
        self.record_cluster_event(
            &existing.cluster_id,
            Some(&existing.host_id),
            Some(instance_id),
            "INSTANCE_REMOVED",
            "WARNING",
            &format!(
                "instance {} removed from the cluster inventory",
                existing.name
            ),
        )
        .await;
        Ok(())
    }

    pub async fn cluster_events_list(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: Option<&str>,
        severity: Option<&str>,
        instance_id: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterEventPage> {
        require_cluster_platform_operator(context)?;
        if let Some(severity) = severity {
            if !CLUSTER_EVENT_SEVERITIES.contains(&severity) {
                return Err(WebServiceError::validation(
                    "severity is not a supported severity",
                ));
            }
        }
        self.repository
            .list_cluster_events(cluster_id, severity, instance_id, page_size, cursor)
            .await
    }

    pub async fn cluster_overview(
        &self,
        context: &WebBackendRequestContext,
    ) -> WebServiceResult<ClusterOverviewResponse> {
        require_cluster_platform_operator(context)?;
        self.repository.cluster_overview().await
    }

    // ------------------------------------------------------------------
    // Machine surface: registration, heartbeat, peer directory
    // ------------------------------------------------------------------

    /// Registers (or re-registers) one webserver process instance and its host
    /// machine. Idempotent per `(host machine code, process pid)`; the latest
    /// registration wins and receives a fresh heartbeat token.
    pub async fn cluster_register(
        &self,
        remote_ip: Option<&str>,
        request: &ClusterRegistrationRequest,
    ) -> WebServiceResult<ClusterRegistrationResponse> {
        validate_registration(request)?;
        // Join mode: `LAN` (same-subnet) vs `TUNNEL` (API-only through the
        // reverse tunnel). A TUNNEL host must advertise its tunnel route
        // domain; a LAN host must not carry a tunnel descriptor.
        let join_mode = request
            .host
            .join_mode
            .as_deref()
            .map(sdkwork_webserver_contract::cluster_join_mode_value)
            .unwrap_or(Some(sdkwork_webserver_contract::CLUSTER_JOIN_MODE_LAN))
            .ok_or_else(|| {
                WebServiceError::Validation(format!(
                    "invalid joinMode `{}`; expected one of {:?}",
                    request.host.join_mode.as_deref().unwrap_or_default(),
                    CLUSTER_JOIN_MODES
                ))
            })?;
        if join_mode == CLUSTER_JOIN_MODE_TUNNEL {
            let has_route = request
                .host
                .tunnel
                .as_ref()
                .is_some_and(|tunnel| !tunnel.route_domain.trim().is_empty());
            if !has_route {
                return Err(WebServiceError::Validation(
                    "TUNNEL hosts must carry a tunnel descriptor with their route domain"
                        .to_owned(),
                ));
            }
        } else if request.host.tunnel.is_some() {
            return Err(WebServiceError::Validation(
                "LAN hosts must not carry a tunnel descriptor".to_owned(),
            ));
        }
        let tunnel_route_domain = request
            .host
            .tunnel
            .as_ref()
            .map(|tunnel| tunnel.route_domain.clone());
        let tunnel_endpoint = request
            .host
            .tunnel
            .as_ref()
            .and_then(|t| t.endpoint.clone());
        let cluster = self
            .repository
            .resolve_cluster_identity(0, request.cluster_code.as_deref())
            .await?;

        let host = self
            .repository
            .upsert_cluster_host(ClusterHostUpsert {
                tenant_id: 0,
                cluster_id: cluster.cluster_id,
                name: host_display_name(request),
                hostname: request.host.hostname.clone(),
                machine_code: request.host.machine_code.clone(),
                os_name: request.host.os_name.clone(),
                os_version: request.host.os_version.clone(),
                kernel_version: request.host.kernel_version.clone(),
                arch: request.host.arch.clone(),
                cpu_model: request.host.cpu_model.clone(),
                cpu_cores: request.host.cpu_cores,
                memory_total_mb: request.host.memory_total_mb,
                remote_ip: remote_ip
                    .map(str::to_owned)
                    .or_else(|| request.host.remote_ip.clone()),
                local_ips: request.host.local_ips.clone(),
                mac_addresses: request.host.mac_addresses.clone(),
                daemon_version: request.host.daemon_version.clone(),
                join_mode,
                tunnel_route_domain: tunnel_route_domain.clone(),
                tunnel_endpoint: tunnel_endpoint.clone(),
            })
            .await?;

        let instance_token = generate_cluster_instance_token();
        let instance = self
            .repository
            .upsert_cluster_instance(ClusterInstanceUpsert {
                tenant_id: 0,
                host_id: host.id,
                cluster_id: cluster.cluster_id,
                name: instance_display_name(request),
                role: request.instance.role.clone(),
                environment: request.instance.environment.clone(),
                process_pid: request.instance.process_pid,
                process_started_at: request.instance.process_started_at.clone(),
                bind_host: request.instance.bind_host.clone(),
                bind_port: request.instance.bind_port,
                public_endpoint: request.instance.public_endpoint.clone(),
                build_version: request
                    .instance
                    .build_version
                    .clone()
                    .or_else(|| request.host.daemon_version.clone()),
                instance_token_hash: hash_cluster_instance_token(&instance_token),
                join_mode: request
                    .instance
                    .join_mode
                    .as_deref()
                    .map(sdkwork_webserver_contract::cluster_join_mode_value)
                    .flatten()
                    .unwrap_or(join_mode),
                tunnel_route_domain: tunnel_route_domain.clone(),
            })
            .await?;

        if host.created {
            self.record_cluster_event(
                &cluster.cluster_uuid,
                Some(&host.uuid),
                None,
                "HOST_REGISTERED",
                "INFO",
                &format!(
                    "host {} joined cluster {}",
                    request.host.hostname, cluster.code
                ),
            )
            .await;
        }
        if instance.created {
            self.record_cluster_event(
                &cluster.cluster_uuid,
                Some(&host.uuid),
                Some(&instance.uuid),
                "INSTANCE_REGISTERED",
                "INFO",
                &format!(
                    "instance {} (pid {}) registered in cluster {}",
                    instance_display_name(request),
                    request.instance.process_pid,
                    cluster.code
                ),
            )
            .await;
        }

        let peers = self
            .repository
            .list_cluster_peers(&instance.uuid, MAX_PEERS_PER_RESPONSE)
            .await?;
        Ok(ClusterRegistrationResponse {
            cluster: sdkwork_webserver_contract::ClusterRef {
                id: cluster.cluster_uuid,
                name: cluster.name,
                code: cluster.code,
            },
            host: sdkwork_webserver_contract::ClusterHostRef {
                id: host.uuid,
                name: host_display_name(request),
                hostname: request.host.hostname.clone(),
            },
            instance: sdkwork_webserver_contract::ClusterInstanceRef {
                id: instance.uuid,
                name: instance_display_name(request),
                role: request.instance.role.clone(),
            },
            instance_token,
            heartbeat_interval_seconds: cluster.heartbeat_interval_seconds,
            offline_threshold_seconds: cluster.offline_threshold_seconds,
            peers,
        })
    }

    /// Records one instance heartbeat and returns the refreshed peer directory
    /// plus any pending mailbox messages (at-most-once handoff).
    pub async fn cluster_heartbeat(
        &self,
        instance_uuid: &str,
        request: &ClusterHeartbeatRequest,
    ) -> WebServiceResult<ClusterHeartbeatResponse> {
        validate_heartbeat(request)?;
        let credentials: ClusterInstanceCredentials = self
            .repository
            .resolve_cluster_instance_by_uuid(instance_uuid)
            .await?;
        let now = now_rfc3339();
        let metrics_json = serde_json::to_string(&request.metrics)
            .map_err(|error| WebServiceError::Internal(format!("encode metrics: {error}")))?;
        // Node service quality: derive the 0..=100 score from the heartbeat
        // quality sample (`metrics.quality`), when the node reports one.
        let quality_score = request
            .metrics
            .get("quality")
            .cloned()
            .map(|value| serde_json::from_value::<ClusterQualityMetrics>(value))
            .transpose()
            .map_err(|error| {
                WebServiceError::Validation(format!("invalid metrics.quality: {error}"))
            })?
            .map(|sample| cluster_quality_score(&sample));
        let transition = self
            .repository
            .record_cluster_heartbeat(ClusterHeartbeatWrite {
                tenant_id: credentials.tenant_id,
                instance_id: credentials.instance_id,
                host_id: credentials.host_id,
                status: request.status,
                health_state: request.health_state.clone(),
                uptime_seconds: request.uptime_seconds,
                build_version: request.build_version.clone(),
                metrics_json,
                reported_at: now.clone(),
                quality_score,
            })
            .await?;
        self.record_heartbeat_transition_events(&credentials, &transition, request)
            .await;

        let messages = self
            .repository
            .claim_cluster_peer_messages(credentials.instance_id, MAX_MESSAGES_PER_HEARTBEAT, &now)
            .await?;
        let peers = self
            .repository
            .list_cluster_peers(instance_uuid, MAX_PEERS_PER_RESPONSE)
            .await?;
        let sync = self.cluster_sync_states(&credentials).await?;
        Ok(ClusterHeartbeatResponse {
            instance_id: credentials.instance_uuid,
            status: request.status,
            acknowledged_at: now,
            heartbeat_interval_seconds: credentials.heartbeat_interval_seconds,
            offline_threshold_seconds: credentials.offline_threshold_seconds,
            peers,
            messages,
            sync,
            ops: Some(sdkwork_webserver_contract::ClusterInstanceOpsDirectives {
                routing_enabled: credentials.routing_enabled,
                drain_requested: credentials.draining,
            }),
        })
    }

    /// Builds the per-track desired-vs-applied sync states for one
    /// instance; `None` when the cluster never published any revision (the
    /// sync plane is inactive).
    async fn cluster_sync_states(
        &self,
        credentials: &ClusterInstanceCredentials,
    ) -> WebServiceResult<Option<Vec<ClusterSyncState>>> {
        let desired = self
            .repository
            .latest_cluster_sync_desired(credentials.tenant_id, credentials.cluster_id)
            .await?;
        if desired.is_empty() {
            return Ok(None);
        }
        let now = now_rfc3339();
        let mut states = Vec::with_capacity(desired.len());
        for track in desired {
            let (applied, desired_revision) = if track.kind == CLUSTER_SYNC_KIND_APPLICATIONS {
                (
                    credentials.applied_applications_revision.clone(),
                    track.revision.clone(),
                )
            } else {
                (
                    credentials.applied_config_revision.clone(),
                    track.revision.clone(),
                )
            };
            let status = if applied.as_deref() == Some(desired_revision.as_str()) {
                CLUSTER_SYNC_STATUS_IN_SYNC
            } else {
                CLUSTER_SYNC_STATUS_PENDING
            };
            states.push(ClusterSyncState {
                kind: sdkwork_webserver_contract::cluster_sync_kind_label(track.kind).to_owned(),
                desired_revision: Some(desired_revision),
                applied_revision: applied,
                status: sdkwork_webserver_contract::cluster_sync_status_label(status).to_owned(),
                updated_at: Some(now.clone()),
            });
        }
        Ok(Some(states))
    }

    /// Serves one sync manifest to a node: the desired revision payload of
    /// `kind` for the instance's cluster (complete data synchronization for
    /// configuration and application management).
    pub async fn cluster_sync_manifest(
        &self,
        instance_uuid: &str,
        kind: &str,
    ) -> WebServiceResult<ClusterSyncManifest> {
        let kind_value = match kind {
            "config" => CLUSTER_SYNC_KIND_CONFIG,
            "applications" => CLUSTER_SYNC_KIND_APPLICATIONS,
            other => {
                return Err(WebServiceError::Validation(format!(
                    "invalid sync kind `{other}`; expected config or applications"
                )))
            }
        };
        let credentials: ClusterInstanceCredentials = self
            .repository
            .resolve_cluster_instance_by_uuid(instance_uuid)
            .await?;
        let desired = self
            .repository
            .latest_cluster_sync_desired(credentials.tenant_id, credentials.cluster_id)
            .await?;
        let track = desired
            .into_iter()
            .find(|track| track.kind == kind_value)
            .ok_or_else(|| WebServiceError::NotFound("cluster sync revision not found".into()))?;
        let payload = self
            .repository
            .cluster_sync_revision_payload(
                credentials.tenant_id,
                credentials.cluster_id,
                kind_value,
                &track.revision,
            )
            .await?
            .ok_or_else(|| WebServiceError::NotFound("cluster sync revision not found".into()))?;
        Ok(ClusterSyncManifest {
            cluster_id: credentials.cluster_uuid,
            kind: kind.to_owned(),
            revision: payload.revision,
            sha256: payload.sha256,
            payload: payload.payload,
            created_at: payload.created_at,
        })
    }

    /// Records one node sync acknowledgment: validates the acknowledged
    /// revision against the desired state, persists the applied revision,
    /// and recomputes the aggregate sync status.
    pub async fn cluster_sync_ack(
        &self,
        instance_uuid: &str,
        request: &ClusterSyncAckRequest,
    ) -> WebServiceResult<ClusterSyncState> {
        let kind_value = match request.kind.as_str() {
            "config" => CLUSTER_SYNC_KIND_CONFIG,
            "applications" => CLUSTER_SYNC_KIND_APPLICATIONS,
            other => {
                return Err(WebServiceError::Validation(format!(
                    "invalid sync kind `{other}`; expected config or applications"
                )))
            }
        };
        let status_value = match request.status.as_str() {
            "IN_SYNC" => CLUSTER_SYNC_STATUS_IN_SYNC,
            "FAILED" => sdkwork_webserver_contract::CLUSTER_SYNC_STATUS_FAILED,
            other => {
                return Err(WebServiceError::Validation(format!(
                    "invalid sync status `{other}`; expected IN_SYNC or FAILED"
                )))
            }
        };
        let credentials: ClusterInstanceCredentials = self
            .repository
            .resolve_cluster_instance_by_uuid(instance_uuid)
            .await?;
        let desired = self
            .repository
            .latest_cluster_sync_desired(credentials.tenant_id, credentials.cluster_id)
            .await?;
        let desired_revision = desired
            .into_iter()
            .find(|track| track.kind == kind_value)
            .map(|track| track.revision)
            .ok_or_else(|| WebServiceError::NotFound("cluster sync revision not found".into()))?;
        if desired_revision != request.revision {
            return Err(WebServiceError::Validation(format!(
                "acknowledged revision {} is not the desired revision {desired_revision}",
                request.revision
            )));
        }
        let now = now_rfc3339();
        self.repository
            .record_cluster_sync_ack(ClusterSyncAckWrite {
                tenant_id: credentials.tenant_id,
                instance_id: credentials.instance_id,
                kind: kind_value,
                applied_revision: request.revision.clone(),
                status: status_value,
                updated_at: now.clone(),
            })
            .await?;
        Ok(ClusterSyncState {
            kind: request.kind.clone(),
            desired_revision: Some(desired_revision),
            applied_revision: Some(request.revision.clone()),
            status: request.status.clone(),
            updated_at: Some(now),
        })
    }

    /// Per-instance heartbeat metric history for the detail page (bounded,
    /// newest first). Also used by availability/SLA views.
    pub async fn cluster_instance_metrics_history(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        limit: i32,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage> {
        require_cluster_platform_operator(context)?;
        if !(1..=500).contains(&limit) {
            return Err(WebServiceError::validation(
                "limit must be between 1 and 500",
            ));
        }
        self.repository
            .list_cluster_heartbeats(instance_id, limit, None)
            .await
    }

    /// Node acknowledges graceful-drain completion (PRD instance management:
    /// the drain loop closes on the registry). Marks the instance stopped and
    /// records the lifecycle event.
    pub async fn cluster_drain_complete(&self, instance_uuid: &str) -> WebServiceResult<()> {
        let credentials: ClusterInstanceCredentials = self
            .repository
            .resolve_cluster_instance_by_uuid(instance_uuid)
            .await?;
        self.repository
            .record_cluster_drain_complete(credentials.tenant_id, instance_uuid)
            .await?;
        self.record_cluster_event(
            &credentials.cluster_uuid,
            Some(&credentials.host_uuid),
            Some(&credentials.instance_uuid),
            "INSTANCE_DRAIN_COMPLETED",
            "INFO",
            &format!(
                "instance {} completed draining and stopped",
                credentials.instance_uuid
            ),
        )
        .await;
        Ok(())
    }

    /// Admin action wrapper with backend context enforcement: publishes one
    /// desired-state revision for a cluster (config or applications track).
    pub async fn cluster_sync_publish(
        &self,
        context: &WebBackendRequestContext,
        cluster_id: &str,
        kind: &str,
        payload: serde_json::Value,
    ) -> WebServiceResult<ClusterSyncManifest> {
        require_cluster_platform_operator(context)?;
        self.cluster_publish_sync_revision(cluster_id, kind, payload, None)
            .await
    }

    /// Admin action: publishes one desired-state revision for a cluster
    /// (config or applications track). Every non-deleted instance of the
    /// cluster flips to PENDING until it acknowledges the new revision.
    pub async fn cluster_publish_sync_revision(
        &self,
        cluster_uuid: &str,
        kind: &str,
        payload: serde_json::Value,
        created_by: Option<&str>,
    ) -> WebServiceResult<ClusterSyncManifest> {
        let kind_value = match kind {
            "config" => CLUSTER_SYNC_KIND_CONFIG,
            "applications" => CLUSTER_SYNC_KIND_APPLICATIONS,
            other => {
                return Err(WebServiceError::Validation(format!(
                    "invalid sync kind `{other}`; expected config or applications"
                )))
            }
        };
        let cluster = self
            .repository
            .resolve_cluster_identity_by_uuid(0, cluster_uuid)
            .await?;
        let revision = self.repository.next_cluster_sync_revision_id().await?;
        let canonical = serde_json::to_vec(&payload)
            .map_err(|error| WebServiceError::Internal(format!("encode payload: {error}")))?;
        let sha256 = sha256_hash(&canonical);
        let size_bytes = i64::try_from(canonical.len()).unwrap_or(i64::MAX);
        let now = now_rfc3339();
        self.repository
            .publish_cluster_sync_revision(ClusterSyncRevisionPublish {
                tenant_id: 0,
                cluster_id: cluster.cluster_id,
                kind: kind_value,
                revision: revision.clone(),
                sha256: sha256.clone(),
                payload,
                size_bytes,
                created_by: created_by.map(str::to_owned),
                created_at: now.clone(),
            })
            .await?;
        Ok(ClusterSyncManifest {
            cluster_id: cluster.cluster_uuid,
            kind: kind.to_owned(),
            revision,
            sha256,
            payload: serde_json::Value::Null,
            created_at: now,
        })
    }

    pub(crate) async fn cluster_peers(
        &self,
        instance_uuid: &str,
    ) -> WebServiceResult<ClusterPeerDirectoryResponse> {
        let peers = self
            .repository
            .list_cluster_peers(instance_uuid, MAX_PEERS_PER_RESPONSE)
            .await?;
        Ok(ClusterPeerDirectoryResponse {
            instance_id: instance_uuid.to_string(),
            peers,
        })
    }

    /// Instance drill-down: stored heartbeat samples, newest first.
    pub async fn cluster_heartbeat_list(
        &self,
        context: &WebBackendRequestContext,
        instance_id: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage> {
        require_cluster_platform_operator(context)?;
        self.repository
            .list_cluster_heartbeats(instance_id, page_size, cursor)
            .await
    }

    /// Enqueues peer messages through the admin surface. A direct message
    /// materializes one row; a broadcast materializes one row per ONLINE
    /// member so every member claims its own copy.
    pub async fn cluster_message_enqueue(
        &self,
        context: &WebBackendRequestContext,
        request: &EnqueueClusterPeerMessagesRequest,
    ) -> WebServiceResult<EnqueueClusterPeerMessagesResponse> {
        require_cluster_platform_operator(context)?;
        validate_bounded_text(&request.message_type, 1, 64, "messageType")?;
        let payload_bytes = serde_json::to_vec(&request.payload)
            .map_err(|_error| WebServiceError::validation("payload must be valid JSON"))?
            .len();
        if payload_bytes > MAX_METRICS_BYTES {
            return Err(WebServiceError::validation(format!(
                "payload must not exceed {MAX_METRICS_BYTES} bytes"
            )));
        }
        let expires_in_seconds = request.expires_in_seconds.unwrap_or(3_600);
        if !(1..=86_400).contains(&expires_in_seconds) {
            return Err(WebServiceError::validation(
                "expiresInSeconds must be between 1 and 86400",
            ));
        }
        let now = now_rfc3339();
        let expires = (Utc::now() + Duration::seconds(expires_in_seconds))
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        let enqueued = self
            .repository
            .enqueue_cluster_peer_messages(ClusterPeerMessageEnqueue {
                tenant_id: 0,
                cluster_uuid: &request.cluster_id,
                from_instance_uuid: request.from_instance_id.as_deref(),
                to_instance_uuid: request.to_instance_id.as_deref(),
                message_type: &request.message_type,
                payload_json: &serde_json::to_string(&request.payload).map_err(|error| {
                    WebServiceError::Internal(format!("encode payload: {error}"))
                })?,
                deliver_at: &now,
                expires_at: &expires,
            })
            .await
            .map_err(|error| match error {
                WebServiceError::NotFound(_) => WebServiceError::not_found("cluster not found"),
                other => other,
            })?;
        Ok(EnqueueClusterPeerMessagesResponse {
            enqueued: i64::try_from(enqueued).unwrap_or(i64::MAX),
        })
    }

    // ------------------------------------------------------------------
    // Liveness sweep (owned by the gateway background task)
    // ------------------------------------------------------------------

    /// Marks silent instances/hosts offline (per-cluster thresholds), expires
    /// undelivered peer messages, and prunes heartbeat samples beyond the
    /// configured retention. Bounded batches keep every tick O(batch).
    pub async fn run_cluster_liveness_sweep(&self) -> WebServiceResult<ClusterSweepReport> {
        let now = now_rfc3339();
        let mut report = ClusterSweepReport::default();

        let expired_instances = self
            .repository
            .expire_stale_cluster_instances(&now, SWEEP_BATCH_LIMIT)
            .await?;
        for expired in &expired_instances {
            self.record_cluster_event(
                &expired.cluster_uuid,
                Some(&expired.host_uuid),
                Some(&expired.instance_uuid),
                "INSTANCE_OFFLINE",
                "WARNING",
                &format!(
                    "instance {} stopped heartbeating and is offline",
                    expired.name
                ),
            )
            .await;
        }
        report.instances_expired = expired_instances.len();

        let expired_hosts = self
            .repository
            .expire_stale_cluster_hosts(&now, SWEEP_BATCH_LIMIT)
            .await?;
        for expired in &expired_hosts {
            self.record_cluster_event(
                &expired.cluster_uuid,
                Some(&expired.host_uuid),
                None,
                "HOST_OFFLINE",
                "WARNING",
                &format!(
                    "host {} has no reachable instances and is offline",
                    expired.name
                ),
            )
            .await;
        }
        report.hosts_expired = expired_hosts.len();

        report.peer_messages_expired = bounded_count(
            self.repository
                .expire_cluster_peer_messages(&now, SWEEP_BATCH_LIMIT)
                .await?,
        );
        let retention_hours = heartbeat_retention_hours();
        let purge_before = (Utc::now() - Duration::hours(retention_hours)).to_rfc3339();
        report.heartbeats_purged = bounded_count(
            self.repository
                .purge_cluster_heartbeats(&purge_before, SWEEP_BATCH_LIMIT)
                .await?,
        );
        Ok(report)
    }

    // ------------------------------------------------------------------
    // Credential resolution (called by the MachineCredentialAuthenticator)
    // ------------------------------------------------------------------

    /// Resolves a `winst_…` heartbeat token into a tenant-0 cluster machine
    /// principal. Unknown tokens return `None` so the resolver rejects them.
    pub(crate) async fn try_authenticate_cluster_instance_token(
        &self,
        token: &str,
    ) -> WebServiceResult<Option<ClusterMachineCredential>> {
        match self
            .repository
            .authenticate_cluster_instance_token(token)
            .await
        {
            Ok(credentials) => Ok(Some(ClusterMachineCredential {
                tenant_id: credentials.tenant_id,
                subject_id: credentials.instance_uuid,
                permission_scope: vec!["web.cluster.*".to_owned()],
            })),
            Err(WebServiceError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Validates the shared registration credential against the configured
    /// `SDKWORK_WEBSERVER_CLUSTER_REGISTRATION_TOKEN` (constant-time compare).
    /// An unconfigured token never validates.
    pub(crate) fn try_authenticate_cluster_registration_token(
        &self,
        token: &str,
    ) -> Option<ClusterMachineCredential> {
        let configured =
            std::env::var("SDKWORK_WEBSERVER_CLUSTER_REGISTRATION_TOKEN").unwrap_or_default();
        if configured.is_empty() || !secure_compare(&configured, token) {
            return None;
        }
        Some(ClusterMachineCredential {
            tenant_id: 0,
            subject_id: CLUSTER_REGISTRATION_SUBJECT.to_owned(),
            permission_scope: vec!["web.cluster.*".to_owned()],
        })
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    async fn record_heartbeat_transition_events(
        &self,
        credentials: &ClusterInstanceCredentials,
        transition: &crate::repository::ClusterHeartbeatTransition,
        request: &ClusterHeartbeatRequest,
    ) {
        let (event_type, severity) = match (transition.previous_status, request.status) {
            (previous, 1) if previous != 1 => ("INSTANCE_ONLINE", "INFO"),
            (1, current) if current != 1 => ("INSTANCE_OFFLINE", "WARNING"),
            _ => match (
                transition.previous_health_state.as_str(),
                request.health_state.as_str(),
            ) {
                (_, "UNHEALTHY") if transition.previous_health_state != "UNHEALTHY" => {
                    ("HEALTH_UNHEALTHY", "ERROR")
                }
                (_, "DEGRADED") if transition.previous_health_state == "HEALTHY" => {
                    ("HEALTH_DEGRADED", "WARNING")
                }
                _ => return,
            },
        };
        self.record_cluster_event(
            &credentials.cluster_uuid,
            Some(&credentials.host_uuid),
            Some(&credentials.instance_uuid),
            event_type,
            severity,
            &format!(
                "instance reported status {} health {}",
                request.status, request.health_state
            ),
        )
        .await;
    }

    /// Writes one cluster event. Persistence failures are logged and swallowed
    /// so monitoring writes never fail a business operation after its durable
    /// effect has committed.
    async fn record_cluster_event(
        &self,
        cluster_uuid: &str,
        host_uuid: Option<&str>,
        instance_uuid: Option<&str>,
        event_type: &str,
        severity: &str,
        message: &str,
    ) {
        let now = now_rfc3339();
        if let Err(error) = self
            .repository
            .record_cluster_event(ClusterEventWrite {
                tenant_id: 0,
                cluster_uuid,
                host_uuid,
                instance_uuid,
                event_type,
                severity,
                message,
                detail_json: "{}",
                occurred_at: &now,
            })
            .await
        {
            tracing::warn!(error = ?error, "failed to persist cluster event {event_type}");
        }
    }
}

/// Second enforcement point for PRD-FR-030 on the cluster plane: the route layer
/// already gated the surface, but the service re-checks so a direct in-process
/// caller cannot bypass it.
///
/// Resolved through [`web_is_platform_operator_tenant`] rather than a literal so
/// this guard, the route guard, and the IAM bootstrap all answer "which tenant
/// is the platform operator" the same way. A previous literal `0` made the
/// cluster plane unreachable in standalone deployments, where the operator
/// tenant is the IAM bootstrap tenant.
///
/// [`web_is_platform_operator_tenant`]: sdkwork_webserver_contract::web_is_platform_operator_tenant
fn require_cluster_platform_operator(context: &WebBackendRequestContext) -> WebServiceResult<()> {
    let tenant_id = context.tenant_id.map(|id| id.to_string());
    if web_is_platform_operator_tenant(tenant_id.as_deref()) {
        Ok(())
    } else {
        Err(WebServiceError::Forbidden)
    }
}

fn host_display_name(request: &ClusterRegistrationRequest) -> String {
    request
        .host
        .name
        .clone()
        .unwrap_or_else(|| request.host.hostname.clone())
}

fn instance_display_name(request: &ClusterRegistrationRequest) -> String {
    request
        .instance
        .name
        .clone()
        .unwrap_or_else(|| format!("{}#{}", request.host.hostname, request.instance.process_pid))
}

fn validate_cluster_thresholds(
    heartbeat_interval_seconds: i32,
    offline_threshold_seconds: i32,
) -> WebServiceResult<()> {
    if !(5..=600).contains(&heartbeat_interval_seconds) {
        return Err(WebServiceError::validation(
            "heartbeatIntervalSeconds must be between 5 and 600",
        ));
    }
    if !(10..=3600).contains(&offline_threshold_seconds) {
        return Err(WebServiceError::validation(
            "offlineThresholdSeconds must be between 10 and 3600",
        ));
    }
    if offline_threshold_seconds < heartbeat_interval_seconds * 2 {
        return Err(WebServiceError::validation(
            "offlineThresholdSeconds must be at least twice heartbeatIntervalSeconds",
        ));
    }
    Ok(())
}

fn validate_registration(request: &ClusterRegistrationRequest) -> WebServiceResult<()> {
    validate_bounded_text(&request.host.hostname, 1, 255, "host.hostname")?;
    validate_bounded_text(&request.host.machine_code, 8, 128, "host.machineCode")?;
    if let Some(name) = &request.host.name {
        validate_bounded_text(name, 1, 100, "host.name")?;
    }
    if !CLUSTER_INSTANCE_ROLES.contains(&request.instance.role.as_str()) {
        return Err(WebServiceError::validation(
            "instance.role is not a supported role",
        ));
    }
    if !CLUSTER_ENVIRONMENTS.contains(&request.instance.environment.as_str()) {
        return Err(WebServiceError::validation(
            "instance.environment is not a supported environment",
        ));
    }
    if request.instance.process_pid <= 0 {
        return Err(WebServiceError::validation(
            "instance.processPid must be a positive process id",
        ));
    }
    chrono::DateTime::parse_from_rfc3339(&request.instance.process_started_at)
        .map_err(|_| WebServiceError::validation("instance.processStartedAt must be RFC 3339"))?;
    if let Some(public_endpoint) = &request.instance.public_endpoint {
        validate_bounded_text(public_endpoint, 1, 255, "instance.publicEndpoint")?;
    }
    validate_address_list(&request.host.local_ips, MAX_LOCAL_IPS, "host.localIps")?;
    validate_address_list(
        &request.host.mac_addresses,
        MAX_MAC_ADDRESSES,
        "host.macAddresses",
    )?;
    if let Some(code) = &request.cluster_code {
        validate_bounded_text(code, 1, 64, "clusterCode")?;
    }
    Ok(())
}

fn validate_heartbeat(request: &ClusterHeartbeatRequest) -> WebServiceResult<()> {
    if !(0..=5).contains(&request.status) {
        return Err(WebServiceError::validation(
            "status must be between 0 and 5",
        ));
    }
    if !CLUSTER_HEALTH_STATES.contains(&request.health_state.as_str()) {
        return Err(WebServiceError::validation(
            "healthState is not a supported health state",
        ));
    }
    if request.uptime_seconds < 0 {
        return Err(WebServiceError::validation("uptimeSeconds must be >= 0"));
    }
    if let Some(build_version) = &request.build_version {
        validate_bounded_text(build_version, 1, 64, "buildVersion")?;
    }
    let metrics_bytes = serde_json::to_vec(&request.metrics)
        .map_err(|_error| WebServiceError::validation("metrics must be valid JSON"))?
        .len();
    if metrics_bytes > MAX_METRICS_BYTES {
        return Err(WebServiceError::validation(format!(
            "metrics must not exceed {MAX_METRICS_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    field: &str,
) -> WebServiceResult<()> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(WebServiceError::validation(format!(
            "{field} must contain {minimum}..{maximum} characters"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(WebServiceError::validation(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn validate_address_list(values: &[String], maximum: usize, field: &str) -> WebServiceResult<()> {
    if values.len() > maximum {
        return Err(WebServiceError::validation(format!(
            "{field} must contain at most {maximum} entries"
        )));
    }
    for value in values {
        validate_bounded_text(value, 1, MAX_ADDRESS_BYTES, field)?;
    }
    Ok(())
}

fn heartbeat_retention_hours() -> i64 {
    std::env::var("SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_RETENTION_HOURS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|hours| (1..=8760).contains(hours))
        .unwrap_or(DEFAULT_HEARTBEAT_RETENTION_HOURS)
}

fn bounded_count(value: u64) -> usize {
    value.min(i64::from(u32::MAX) as u64) as usize
}

pub(crate) fn hash_cluster_instance_token(token: &str) -> String {
    sha256_hash(token.as_bytes())
}

pub(crate) fn generate_cluster_instance_token() -> String {
    format!("{}{}", CLUSTER_INSTANCE_TOKEN_PREFIX, uuid_v4())
}

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_contract::web_platform_operator_tenant_id;

    #[test]
    fn cluster_thresholds_must_allow_two_missed_heartbeats() {
        assert!(validate_cluster_thresholds(15, 60).is_ok());
        assert!(validate_cluster_thresholds(60, 60).is_err());
        assert!(validate_cluster_thresholds(0, 60).is_err());
        assert!(validate_cluster_thresholds(15, 8).is_err());
    }

    fn cluster_context(tenant_id: Option<i64>) -> WebBackendRequestContext {
        WebBackendRequestContext {
            operator_id: Some(1),
            tenant_id,
            subject_id: Some("1".to_owned()),
            idempotency_key: None,
            permission_scope: vec!["web.cluster.read".to_owned()],
        }
    }

    /// PRD-FR-030 on the cluster plane: the platform operator tenant is accepted
    /// and any other tenant is forbidden. The positive half guards against the
    /// regression where this compared against the literal `0` — no tenant is
    /// ever `0`, which made every cluster operation unreachable.
    #[test]
    fn cluster_plane_admits_only_the_platform_operator_tenant() {
        let operator_tenant = web_platform_operator_tenant_id()
            .parse()
            .expect("platform operator tenant id is numeric");
        assert!(require_cluster_platform_operator(&cluster_context(Some(operator_tenant))).is_ok());
        assert!(
            require_cluster_platform_operator(&cluster_context(Some(operator_tenant + 1))).is_err()
        );
        assert!(require_cluster_platform_operator(&cluster_context(Some(0))).is_err());
        assert!(require_cluster_platform_operator(&cluster_context(None)).is_err());
        assert!(
            matches!(
                require_cluster_platform_operator(&cluster_context(None)),
                Err(WebServiceError::Forbidden)
            ),
            "a non-operator tenant must be forbidden, not merely rejected"
        );
    }

    #[test]
    fn heartbeat_rejects_unknown_health_states() {
        let mut request = ClusterHeartbeatRequest {
            status: 1,
            health_state: "HEALTHY".to_string(),
            uptime_seconds: 12,
            build_version: None,
            metrics: serde_json::json!({"cpu": 0.1}),
        };
        assert!(validate_heartbeat(&request).is_ok());
        request.health_state = "FINE".to_string();
        assert!(validate_heartbeat(&request).is_err());
        request.health_state = "HEALTHY".to_string();
        request.status = 9;
        assert!(validate_heartbeat(&request).is_err());
    }

    #[test]
    fn registration_rejects_unknown_roles_and_bad_identity() {
        let raw = r#"{
            "host": {"hostname": "edge-1", "machineCode": "mc-12345678"},
            "instance": {
                "role": "GATEWAY",
                "environment": "production",
                "processPid": 42,
                "processStartedAt": "2026-09-19T00:00:00Z"
            }
        }"#;
        let request: ClusterRegistrationRequest = serde_json::from_str(raw).unwrap();
        assert!(validate_registration(&request).is_ok());

        let mut bad = request.clone();
        bad.instance.role = "PRIMARY".to_string();
        assert!(validate_registration(&bad).is_err());

        bad.instance.role = "GATEWAY".to_string();
        bad.instance.process_pid = 0;
        assert!(validate_registration(&bad).is_err());
    }

    #[test]
    fn constant_time_compare_comes_from_the_shared_crypto_helper() {
        assert!(secure_compare("wreg_abc", "wreg_abc"));
        assert!(!secure_compare("wreg_abc", "wreg_abd"));
    }

    #[test]
    fn instance_tokens_use_the_winst_prefix() {
        let token = generate_cluster_instance_token();
        assert!(token.starts_with(CLUSTER_INSTANCE_TOKEN_PREFIX));
        let hash = hash_cluster_instance_token(&token);
        assert_eq!(hash.len(), 64);
    }
}
