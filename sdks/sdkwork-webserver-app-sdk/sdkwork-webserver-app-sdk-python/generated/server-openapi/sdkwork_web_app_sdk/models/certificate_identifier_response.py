from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CertificateIdentifierResponse:
    domain_id: str
    hostname: str
    identifier_type: str
    position: int
