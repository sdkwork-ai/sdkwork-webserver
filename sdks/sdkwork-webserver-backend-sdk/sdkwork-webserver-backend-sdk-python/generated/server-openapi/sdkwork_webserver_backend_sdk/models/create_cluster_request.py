from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateClusterRequest:
    name: str
    code: str
    description: Optional[str] = None
    heartbeat_interval_seconds: Optional[int] = None
    offline_threshold_seconds: Optional[int] = None
    lb_strategy: Optional[str] = None
    served_domains: Optional[List[str]] = None
