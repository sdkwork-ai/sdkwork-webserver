from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainResponse:
    id: str
    hostname: str
    certificate_count: str
    is_primary: bool
    is_verified: bool
    ssl_enabled: bool
    status: int
    created_at: str
    application_id: Optional[str] = None
    application_name: Optional[str] = None
    ssl_provider: Optional[str] = None
