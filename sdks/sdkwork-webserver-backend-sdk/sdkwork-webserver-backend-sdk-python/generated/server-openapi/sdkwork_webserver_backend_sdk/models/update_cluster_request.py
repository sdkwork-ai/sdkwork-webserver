from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateClusterRequest:
    name: Optional[str] = None
    description: Optional[str] = None
    status: Optional[int] = None
    heartbeat_interval_seconds: Optional[int] = None
    offline_threshold_seconds: Optional[int] = None
