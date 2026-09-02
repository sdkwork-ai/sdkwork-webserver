from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateEnvVariableRequest:
    value: str
    is_secret: Optional[bool] = None
