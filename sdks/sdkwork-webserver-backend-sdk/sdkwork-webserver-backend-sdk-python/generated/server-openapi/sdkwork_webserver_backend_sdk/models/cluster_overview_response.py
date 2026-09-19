from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterOverviewResponse:
    total_hosts: str
    online_hosts: str
    total_instances: str
    online_instances: str
    unhealthy_instances: str
    pending_peer_messages: str
    generated_at: str
