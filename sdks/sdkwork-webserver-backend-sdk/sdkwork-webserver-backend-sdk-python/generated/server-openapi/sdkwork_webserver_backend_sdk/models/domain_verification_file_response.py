from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainVerificationFileResponse:
    id: str
    root_domain_id: str
    platform: str
    file_name: str
    content: str
    created_at: str
    updated_at: str
