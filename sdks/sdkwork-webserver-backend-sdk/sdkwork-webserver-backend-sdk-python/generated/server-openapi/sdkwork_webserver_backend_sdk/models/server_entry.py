from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ServerEntry:
    name: str
    kind: str
    path: str
    size: Optional[str] = None
    project_type: Optional[str] = None
    is_project_root: Optional[bool] = None
