from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class TunnelMachineRouteResponse:
    route_id: str
    name: str
    protocol: str
    allow_public: bool
    auth: str
    healthy: bool
    domain: Optional[str] = None
    port: Optional[int] = None
    secret_name: Optional[str] = None
    locations: Optional[List[str]] = None
    load_balancer_group: Optional[str] = None
    bandwidth_limit_bytes_per_second: Optional[str] = None
    proxy_protocol: Optional[str] = None
    allowed_ips: Optional[List[str]] = None
