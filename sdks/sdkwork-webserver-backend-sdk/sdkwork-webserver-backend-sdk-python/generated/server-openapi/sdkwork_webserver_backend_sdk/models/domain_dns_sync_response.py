from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainDnsSyncResponse:
    record_count: str
    synced_at: str
    zone_apex: str
    dns_provider: str
    cloud_account_id: str
