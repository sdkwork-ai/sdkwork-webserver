from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateListenerCertificateBindingRequest:
    certificate_id: str
    certificate_version_id: Optional[str] = None
    priority: Optional[int] = None
    is_default: Optional[bool] = None
