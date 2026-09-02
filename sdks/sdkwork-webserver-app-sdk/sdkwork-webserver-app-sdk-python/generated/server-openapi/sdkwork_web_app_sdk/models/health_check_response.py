from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class HealthCheckResponse:
    id: str
    check_type: int
    check_url: str
    check_interval: int
    timeout_ms: int
    retry_count: int
    status: int
    created_at: str
