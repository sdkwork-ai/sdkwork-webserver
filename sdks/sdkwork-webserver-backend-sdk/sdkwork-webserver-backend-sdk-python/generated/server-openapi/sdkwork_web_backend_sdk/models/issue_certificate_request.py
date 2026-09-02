from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class IssueCertificateRequest:
    domain_ids: List[str]
    cert_type: int
    key_algorithm: Optional[str] = None
    auto_renew: Optional[bool] = None
