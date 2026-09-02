from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CertificateDistributionResponse:
    server_id: str
    server_name: str
    host: str
    desired_sync_version: str
    status: str
    applied_sync_version: Optional[str] = None
    last_heartbeat_at: Optional[str] = None
