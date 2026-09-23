from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateClusterInstanceRequest:
    name: Optional[str] = None
    status: Optional[int] = None
    public_endpoint: Optional[str] = None
    routing_enabled: Optional[bool] = None
    draining: Optional[bool] = None
    probe_url: Optional[str] = None
    labels: Optional[Dict[str, str]] = None
    routing_weight: Optional[int] = None
    maintenance_note: Optional[str] = None
