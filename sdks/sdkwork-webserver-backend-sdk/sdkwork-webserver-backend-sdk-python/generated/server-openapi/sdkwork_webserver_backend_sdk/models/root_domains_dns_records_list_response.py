from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .domain_dns_record_response import DomainDnsRecordResponse
    from .page_info import PageInfo


@dataclass
class RootDomainsDnsRecordsListResponse:
    code: int
    data: Any
    trace_id: str
