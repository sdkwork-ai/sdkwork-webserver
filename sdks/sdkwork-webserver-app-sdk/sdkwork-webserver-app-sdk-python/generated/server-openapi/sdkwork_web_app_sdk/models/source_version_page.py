from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .source_version_response import SourceVersionResponse


@dataclass
class SourceVersionPage:
    items: Optional[List[SourceVersionResponse]] = None
    total: Optional[str] = None
