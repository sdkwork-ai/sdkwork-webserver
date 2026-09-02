from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .audit_log_response import AuditLogResponse
    from .page_info import PageInfo


@dataclass
class AuditLogsListResponse:
    code: int
    data: Any
    trace_id: str
