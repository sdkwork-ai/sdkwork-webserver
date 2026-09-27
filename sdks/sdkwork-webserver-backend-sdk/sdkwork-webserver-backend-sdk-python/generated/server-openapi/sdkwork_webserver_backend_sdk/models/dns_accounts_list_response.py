from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .dns_account_response import DnsAccountResponse
    from .page_info import PageInfo


@dataclass
class DnsAccountsListResponse:
    code: int
    data: Any
    trace_id: str
