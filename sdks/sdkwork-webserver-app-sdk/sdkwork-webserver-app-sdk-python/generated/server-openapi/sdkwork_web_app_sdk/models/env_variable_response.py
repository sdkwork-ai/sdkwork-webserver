from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class EnvVariableResponse:
    id: Optional[str] = None
    key: Optional[str] = None
    environment: Optional[str] = None
    is_secret: Optional[bool] = None
    created_at: Optional[str] = None
