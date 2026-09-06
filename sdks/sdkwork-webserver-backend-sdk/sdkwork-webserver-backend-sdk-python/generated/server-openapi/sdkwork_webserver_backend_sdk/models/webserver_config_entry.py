from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class WebserverConfigEntry:
    id: str
    kind: str
    name: str
    path: str
    language: str
    writable: bool
    size: Optional[str] = None
    updated_at: Optional[str] = None
