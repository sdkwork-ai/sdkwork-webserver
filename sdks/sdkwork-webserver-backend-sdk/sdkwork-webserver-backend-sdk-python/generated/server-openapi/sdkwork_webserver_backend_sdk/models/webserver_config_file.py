from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class WebserverConfigFile:
    id: str
    kind: str
    name: str
    path: str
    language: str
    writable: bool
    content: str
    size: str
    sha256: str
    updated_at: Optional[str] = None
