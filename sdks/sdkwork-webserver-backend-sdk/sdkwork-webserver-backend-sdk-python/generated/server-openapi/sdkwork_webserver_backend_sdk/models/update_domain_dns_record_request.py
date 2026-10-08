from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateDomainDnsRecordRequest:
    record_type: str
    host: str
    record_value: str
    ttl_seconds: Optional[int] = None
    priority: Optional[int] = None
    record_line: Optional[str] = None
