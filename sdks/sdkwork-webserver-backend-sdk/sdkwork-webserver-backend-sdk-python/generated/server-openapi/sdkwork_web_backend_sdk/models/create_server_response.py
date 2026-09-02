from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateServerResponse:
    id: str
    name: str
    host: str
    tenant_scope_hash: str
    ssh_port: int
    status: int
    created_at: str
    agent_token: str
    last_heartbeat_at: Optional[str] = None
