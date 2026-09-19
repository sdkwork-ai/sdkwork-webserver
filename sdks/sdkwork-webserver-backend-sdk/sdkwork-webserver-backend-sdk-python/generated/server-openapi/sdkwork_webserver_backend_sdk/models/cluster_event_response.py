from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterEventResponse:
    id: str
    cluster_id: str
    event_type: str
    severity: str
    message: str
    detail: Dict[str, Any]
    occurred_at: str
    created_at: str
    host_id: Optional[str] = None
    instance_id: Optional[str] = None
