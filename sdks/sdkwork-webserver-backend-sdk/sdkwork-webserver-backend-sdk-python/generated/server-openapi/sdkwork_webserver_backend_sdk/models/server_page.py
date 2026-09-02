from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_response import ServerResponse


@dataclass
class ServerPage:
    items: Optional[List[ServerResponse]] = None
    total: Optional[str] = None
