from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxConfigRevisionSummary:
    version_no: Optional[int] = None
    config_name: Optional[str] = None
    config_hash: Optional[str] = None
    created_by: Optional[str] = None
    created_at: Optional[str] = None
