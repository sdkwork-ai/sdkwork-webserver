from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .listener_certificate_summary_response import ListenerCertificateSummaryResponse


@dataclass
class ListenerCertificateBindingResponse:
    id: str
    site_id: str
    domain_id: str
    certificate_id: str
    desired_certificate_version_id: str
    desired_certificate: ListenerCertificateSummaryResponse
    key_algorithm: str
    priority: int
    is_default: bool
    status: str
    created_at: str
    updated_at: str
    current_certificate_version_id: Optional[str] = None
    current_certificate: Optional[ListenerCertificateSummaryResponse] = None
    activated_at: Optional[str] = None
