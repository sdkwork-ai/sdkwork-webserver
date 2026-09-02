from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .certificate_identifier_response import CertificateIdentifierResponse


@dataclass
class CertificateResponse:
    id: str
    cert_name: str
    identifiers: List[CertificateIdentifierResponse]
    key_algorithm: str
    status: str
    created_at: str
    cert_type: Optional[int] = None
    issuer: Optional[str] = None
    fingerprint: Optional[str] = None
    not_before: Optional[str] = None
    not_after: Optional[str] = None
    auto_renew: Optional[bool] = None
    renewal_status: Optional[str] = None
