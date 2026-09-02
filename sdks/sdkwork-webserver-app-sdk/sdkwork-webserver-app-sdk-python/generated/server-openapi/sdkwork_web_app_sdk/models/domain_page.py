from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .domain_response import DomainResponse


@dataclass
class DomainPage:
    items: Optional[List[DomainResponse]] = None
    total: Optional[str] = None
