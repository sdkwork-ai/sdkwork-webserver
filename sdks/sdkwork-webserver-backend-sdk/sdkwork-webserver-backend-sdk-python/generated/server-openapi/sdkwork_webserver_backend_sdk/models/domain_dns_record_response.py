from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainDnsRecordResponse:
    id: str
    record_name: str
    host: str
    record_type: str
    record_value: str
    record_status: str
    dns_provider: str
    cloud_account_id: str
    synced_at: str
    ttl_seconds: Optional[int] = None
    priority: Optional[int] = None
    record_line: Optional[str] = None
    domain_id: Optional[str] = None
    provider_record_ref: Optional[str] = None
