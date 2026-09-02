from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxValidateResponse:
    valid: Optional[bool] = None
    errors: Optional[List[Dict[str, Any]]] = None
