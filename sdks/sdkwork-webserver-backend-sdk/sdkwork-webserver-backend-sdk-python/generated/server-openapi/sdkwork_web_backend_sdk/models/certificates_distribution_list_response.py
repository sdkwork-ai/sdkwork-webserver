from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .certificate_distribution_response import CertificateDistributionResponse
    from .page_info import PageInfo


@dataclass
class CertificatesDistributionListResponse:
    code: int
    data: Any
    trace_id: str
