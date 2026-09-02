from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .certificate_operation_response import CertificateOperationResponse


@dataclass
class CertificatesOperationsRetrieveResponse:
    code: int
    data: Any
    trace_id: str
