from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class EnqueueClusterPeerMessagesRequest:
    cluster_id: str
    message_type: str
    to_instance_id: Optional[str] = None
    from_instance_id: Optional[str] = None
    payload: Optional[Dict[str, Any]] = None
    expires_in_seconds: Optional[int] = None
