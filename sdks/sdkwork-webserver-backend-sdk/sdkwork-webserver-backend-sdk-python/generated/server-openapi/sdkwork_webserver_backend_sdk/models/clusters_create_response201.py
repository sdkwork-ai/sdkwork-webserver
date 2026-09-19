from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .cluster_response import ClusterResponse


@dataclass
class ClustersCreateResponse201:
    code: int
    data: Any
    trace_id: str
