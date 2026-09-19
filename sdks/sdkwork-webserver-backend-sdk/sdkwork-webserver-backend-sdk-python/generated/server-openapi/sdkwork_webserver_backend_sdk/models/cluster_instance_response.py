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
