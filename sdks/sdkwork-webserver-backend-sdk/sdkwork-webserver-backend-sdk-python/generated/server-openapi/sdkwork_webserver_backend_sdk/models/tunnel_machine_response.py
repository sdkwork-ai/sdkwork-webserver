from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .tunnel_machine_route_response import TunnelMachineRouteResponse


@dataclass
class TunnelMachineResponse:
    device_id: str
    device_name: str
    platform: str
    online: bool
    enabled: bool
    first_seen_at: str
    last_seen_at: str
    bytes_in: str
    bytes_out: str
    sessions_total: str
    route_count: str
    healthy_route_count: str
    session_id: Optional[str] = None
    connected_at: Optional[str] = None
    disconnected_at: Optional[str] = None
    remote_addr: Optional[str] = None
    unhealthy_route_ids: Optional[List[str]] = None
    routes: Optional[List[TunnelMachineRouteResponse]] = None
