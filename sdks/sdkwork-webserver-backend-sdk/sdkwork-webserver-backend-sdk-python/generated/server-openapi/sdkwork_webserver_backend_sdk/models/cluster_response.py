from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterResponse:
    id: str
    name: str
    code: str
    status: int
    heartbeat_interval_seconds: int
    offline_threshold_seconds: int
    host_count: str
    instance_count: str
    online_instance_count: str
    created_at: str
    updated_at: str
    description: Optional[str] = None
