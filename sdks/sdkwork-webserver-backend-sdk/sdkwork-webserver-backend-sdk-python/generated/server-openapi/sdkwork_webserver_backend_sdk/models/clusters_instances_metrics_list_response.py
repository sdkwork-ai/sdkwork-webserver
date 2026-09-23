from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .cluster_heartbeat_sample_response import ClusterHeartbeatSampleResponse
    from .page_info import PageInfo


@dataclass
class ClustersInstancesMetricsListResponse:
    code: int
    data: Any
    trace_id: str
