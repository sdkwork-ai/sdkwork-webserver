from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterProbeRunResponse:
    healthy: bool
    latency_ms: int
    failures: int
    ejected: bool
    recovered: bool
