from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterHostResponse:
    id: str
    cluster_id: str
    name: str
    hostname: str
    machine_code: str
    local_ips: List[str]
    mac_addresses: List[str]
    status: int
    instance_count: str
    created_at: str
    updated_at: str
    os_name: Optional[str] = None
    os_version: Optional[str] = None
    kernel_version: Optional[str] = None
    arch: Optional[str] = None
    cpu_model: Optional[str] = None
    cpu_cores: Optional[int] = None
    memory_total_mb: Optional[str] = None
    remote_ip: Optional[str] = None
    daemon_version: Optional[str] = None
    last_heartbeat_at: Optional[str] = None
    join_mode: Optional[str] = None
    tunnel_route_domain: Optional[str] = None
