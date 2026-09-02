from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class AuditLogResponse:
    id: Optional[str] = None
    operator_id: Optional[str] = None
    operator_type: Optional[str] = None
    action: Optional[str] = None
    target_type: Optional[str] = None
    target_id: Optional[str] = None
    target_uuid: Optional[str] = None
    ip_address: Optional[str] = None
    changes: Optional[Dict[str, Any]] = None
    created_at: Optional[str] = None
