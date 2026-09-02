from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class AgentCertificateObservation:
    certificate_id: str
    fingerprint: str
    sync_version: str
    state: str
    observed_at: str
    failure_code: Optional[str] = None
