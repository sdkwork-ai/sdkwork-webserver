from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateEnvVariableRequest:
    key: str
    value: str
    environment: Optional[str] = None
    is_secret: Optional[bool] = None
