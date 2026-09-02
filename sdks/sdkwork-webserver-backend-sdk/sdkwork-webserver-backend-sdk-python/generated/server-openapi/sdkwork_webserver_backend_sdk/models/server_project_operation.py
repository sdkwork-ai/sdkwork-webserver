from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ServerProjectOperation:
    id: str
    kind: str
    label: str
    permission: str
    description: Optional[str] = None
    dangerous: Optional[bool] = None
