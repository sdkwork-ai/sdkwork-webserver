from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxReloadResponse:
    success: Optional[bool] = None
    message: Optional[str] = None
    timestamp: Optional[str] = None
