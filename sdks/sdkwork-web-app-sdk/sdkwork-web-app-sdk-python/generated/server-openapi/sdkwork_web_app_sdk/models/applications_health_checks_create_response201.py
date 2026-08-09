from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .health_check_response import HealthCheckResponse


@dataclass
class ApplicationsHealthChecksCreateResponse201:
    code: int
    data: Any
    trace_id: str
