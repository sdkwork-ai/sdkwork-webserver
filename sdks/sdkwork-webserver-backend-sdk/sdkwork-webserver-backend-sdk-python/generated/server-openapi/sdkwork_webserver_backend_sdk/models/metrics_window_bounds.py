from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class MetricsWindowBounds:
    window: str
    date_to: str
    date_from: Optional[str] = None
