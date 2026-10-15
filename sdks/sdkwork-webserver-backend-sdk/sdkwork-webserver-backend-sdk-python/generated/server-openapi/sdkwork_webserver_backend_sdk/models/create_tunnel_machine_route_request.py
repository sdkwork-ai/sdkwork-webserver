from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateTunnelMachineRouteRequest:
    name: str
    protocol: str
    domain: Optional[str] = None
    port: Optional[int] = None
    allow_public: Optional[bool] = None
    allowed_ips: Optional[List[str]] = None
    auth: Optional[str] = None
    visitor_tokens: Optional[List[str]] = None
    locations: Optional[List[str]] = None
    bandwidth_limit_bytes_per_second: Optional[str] = None
    proxy_protocol: Optional[str] = None
    load_balancer_group: Optional[str] = None
