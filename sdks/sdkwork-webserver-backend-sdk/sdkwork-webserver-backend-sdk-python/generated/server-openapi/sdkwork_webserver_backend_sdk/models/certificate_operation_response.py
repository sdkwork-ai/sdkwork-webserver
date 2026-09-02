from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CertificateOperationResponse:
    id: str
    certificate_id: str
    operation_type: str
    status: str
    attempt_count: int
    max_attempts: int
    next_attempt_at: str
    created_at: str
    updated_at: str
    failure_code: Optional[str] = None
    completed_at: Optional[str] = None
