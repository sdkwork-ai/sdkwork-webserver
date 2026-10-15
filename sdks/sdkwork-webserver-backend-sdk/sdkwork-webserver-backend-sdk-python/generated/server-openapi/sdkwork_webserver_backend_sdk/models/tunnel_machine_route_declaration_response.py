from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class TunnelMachineRouteDeclarationResponse:
    route_id: str
    active: bool
    public_url: Optional[str] = None
    detail: Optional[str] = None
