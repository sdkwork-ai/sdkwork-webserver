from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainWechatVerificationResponse:
    root_domain_id: str
    file_name: Optional[str] = None
    content: Optional[str] = None
    updated_at: Optional[str] = None
