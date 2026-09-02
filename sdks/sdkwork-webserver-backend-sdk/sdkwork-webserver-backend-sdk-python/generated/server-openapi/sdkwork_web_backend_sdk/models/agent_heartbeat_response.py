from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class AgentHeartbeatResponse:
    server_id: str
    status: int
    acknowledged_at: str
