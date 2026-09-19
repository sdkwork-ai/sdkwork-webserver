from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterHeartbeatSampleResponse:
    id: str
    status: int
    metrics: Dict[str, Any]
    reported_at: str
    latency_ms: Optional[int] = None
