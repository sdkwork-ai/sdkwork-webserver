from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .agent_sync_response import AgentSyncResponse


@dataclass
class RetrieveResponse:
    code: int
    data: Any
    trace_id: str
