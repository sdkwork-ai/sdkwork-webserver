from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterInstanceResponse:
    id: str
    cluster_id: str
    host_id: str
    name: str
    role: str
    environment: str
    status: int
    health_state: str
    uptime_seconds: str
    metrics: Dict[str, Any]
    created_at: str
    updated_at: str
    host_name: Optional[str] = None
    process_pid: Optional[int] = None
    process_started_at: Optional[str] = None
    bind_host: Optional[str] = None
    bind_port: Optional[int] = None
    public_endpoint: Optional[str] = None
    build_version: Optional[str] = None
    last_heartbeat_at: Optional[str] = None
    last_online_at: Optional[str] = None
    join_mode: Optional[str] = None
    quality_score: Optional[int] = None
    desired_config_revision: Optional[str] = None
    applied_config_revision: Optional[str] = None
    desired_applications_revision: Optional[str] = None
    applied_applications_revision: Optional[str] = None
    sync_status: Optional[str] = None
    routing_enabled: Optional[bool] = None
    draining: Optional[bool] = None
    ejected: Optional[bool] = None
    restart_count: Optional[int] = None
    labels: Optional[Dict[str, str]] = None
    routing_weight: Optional[int] = None
    maintenance_note: Optional[str] = None
    probe_failures: Optional[int] = None
    probe_url: Optional[str] = None
