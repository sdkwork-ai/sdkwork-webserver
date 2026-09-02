from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .audit_log_response import AuditLogResponse


@dataclass
class AuditLogPage:
    items: Optional[List[AuditLogResponse]] = None
    total: Optional[str] = None
