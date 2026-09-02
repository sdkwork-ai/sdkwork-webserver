from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class DomainDeploymentResponse:
    id: str
    status: int
    environment: str
    created_at: str
    version_tag: Optional[str] = None
    completed_at: Optional[str] = None
