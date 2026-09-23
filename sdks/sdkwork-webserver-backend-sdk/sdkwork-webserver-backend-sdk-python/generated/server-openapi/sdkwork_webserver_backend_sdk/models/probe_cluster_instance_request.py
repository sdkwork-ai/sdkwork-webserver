from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ProbeClusterInstanceRequest:
    path: Optional[str] = None
    timeout_ms: Optional[int] = None
