from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateManagedDomainRequest:
    hostname: str
    application_id: Optional[str] = None
    is_primary: Optional[bool] = None
    ssl_enabled: Optional[bool] = None
    ssl_provider: Optional[str] = None
