from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .certificate_identifier_response import CertificateIdentifierResponse


@dataclass
class ListenerCertificateSummaryResponse:
    cert_name: str
    identifiers: List[CertificateIdentifierResponse]
    status: str
    issuer: Optional[str] = None
    fingerprint: Optional[str] = None
    not_after: Optional[str] = None
