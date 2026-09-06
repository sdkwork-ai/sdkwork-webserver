from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class WebserverConfigWriteRequest:
    content: str
    expected_sha256: Optional[str] = None
