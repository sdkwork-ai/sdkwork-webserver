from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .cluster_response import ClusterResponse
    from .page_info import PageInfo


@dataclass
class ClustersListResponse:
    code: int
    data: Any
    trace_id: str
