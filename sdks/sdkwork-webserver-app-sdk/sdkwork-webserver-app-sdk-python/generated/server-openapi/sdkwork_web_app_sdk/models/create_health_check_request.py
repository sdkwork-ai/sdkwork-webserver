from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateHealthCheckRequest:
    check_type: int
    check_url: str
    check_interval: Optional[int] = None
    timeout_ms: Optional[int] = None
    retry_count: Optional[int] = None
