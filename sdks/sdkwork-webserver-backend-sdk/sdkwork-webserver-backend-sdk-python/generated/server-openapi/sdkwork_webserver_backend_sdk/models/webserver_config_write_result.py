from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class WebserverConfigWriteResult:
    id: str
    path: str
    size: str
    sha256: str
    updated_at: str
    backup_path: Optional[str] = None
