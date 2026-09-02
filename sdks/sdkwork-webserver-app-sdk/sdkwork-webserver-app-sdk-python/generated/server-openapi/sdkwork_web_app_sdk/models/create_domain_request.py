from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateDomainRequest:
    hostname: str
    is_primary: Optional[bool] = None
    ssl_enabled: Optional[bool] = None
    ssl_provider: Optional[str] = None
