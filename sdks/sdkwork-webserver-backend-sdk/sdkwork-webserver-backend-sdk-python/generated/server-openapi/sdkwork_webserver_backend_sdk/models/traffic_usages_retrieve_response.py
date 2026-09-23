from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .traffic_usage_statistics_response import TrafficUsageStatisticsResponse


@dataclass
class TrafficUsagesRetrieveResponse:
    code: int
    data: Any
    trace_id: str
