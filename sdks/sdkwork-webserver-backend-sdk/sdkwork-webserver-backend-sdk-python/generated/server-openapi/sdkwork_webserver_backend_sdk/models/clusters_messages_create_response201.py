from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .enqueue_cluster_peer_messages_response import EnqueueClusterPeerMessagesResponse


@dataclass
class ClustersMessagesCreateResponse201:
    code: int
    data: Any
    trace_id: str
