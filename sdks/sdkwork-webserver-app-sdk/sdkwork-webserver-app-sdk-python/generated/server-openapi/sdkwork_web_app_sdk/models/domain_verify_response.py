from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainVerifyResponse:
    verified: bool
    status: str
    method: str
    record_name: str
    record_value: str
    attempt_count: int
    expires_at: str
    next_attempt_at: Optional[str] = None
    checked_at: Optional[str] = None
    failure_code: Optional[str] = None
