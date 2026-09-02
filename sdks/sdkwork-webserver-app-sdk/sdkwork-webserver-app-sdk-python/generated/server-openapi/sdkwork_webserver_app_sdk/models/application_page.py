from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_response import ApplicationResponse


@dataclass
class ApplicationPage:
    items: Optional[List[ApplicationResponse]] = None
    total: Optional[str] = None
    page: Optional[int] = None
    page_size: Optional[int] = None
