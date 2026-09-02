from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .agent_heartbeat_response import AgentHeartbeatResponse


@dataclass
class HeartbeatResponse:
    code: int
    data: Any
    trace_id: str
