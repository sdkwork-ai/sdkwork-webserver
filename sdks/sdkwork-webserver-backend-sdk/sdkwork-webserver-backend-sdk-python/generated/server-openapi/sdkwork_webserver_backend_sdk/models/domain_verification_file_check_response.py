from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainVerificationFileCheckResponse:
    reachable: bool
    matched: bool
    checked_at: str
    status_code: Optional[int] = None
    scheme: Optional[str] = None
    detail: Optional[str] = None
