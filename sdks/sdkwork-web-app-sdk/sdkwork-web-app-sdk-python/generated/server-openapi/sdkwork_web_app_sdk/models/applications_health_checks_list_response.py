from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .health_check_response import HealthCheckResponse
    from .page_info import PageInfo


@dataclass
class ApplicationsHealthChecksListResponse:
    code: int
    data: Any
    trace_id: str
