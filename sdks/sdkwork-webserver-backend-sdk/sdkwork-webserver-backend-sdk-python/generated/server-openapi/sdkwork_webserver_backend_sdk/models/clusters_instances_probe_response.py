from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .cluster_probe_run_response import ClusterProbeRunResponse


@dataclass
class ClustersInstancesProbeResponse:
    code: int
    data: Any
    trace_id: str
