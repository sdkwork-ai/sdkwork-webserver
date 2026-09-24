from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .metrics_summary_response import MetricsSummaryResponse


@dataclass
class MetricsSummariesRetrieveResponse:
    code: int
    data: Any
    trace_id: str
