from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class IssueCertificateRequest:
    domain_ids: List[str]
    cert_type: int
    key_algorithm: Optional[str] = None
    auto_renew: Optional[bool] = None
    cert_name: Optional[str] = None
    certificate_scope: Optional[str] = None
    validation_method: Optional[str] = None
    renew_before_days: Optional[int] = None
    ca_profile: Optional[str] = None
    provider_account_id: Optional[str] = None
