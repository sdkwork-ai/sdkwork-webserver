from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .certificate_response import CertificateResponse
    from .page_info import PageInfo


@dataclass
class CertificatesListResponse:
    code: int
    data: Any
    trace_id: str
