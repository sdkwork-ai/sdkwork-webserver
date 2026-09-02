from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .nginx_status_response import NginxStatusResponse


@dataclass
class StatusRetrieveResponse:
    code: int
    data: Any
    trace_id: str
