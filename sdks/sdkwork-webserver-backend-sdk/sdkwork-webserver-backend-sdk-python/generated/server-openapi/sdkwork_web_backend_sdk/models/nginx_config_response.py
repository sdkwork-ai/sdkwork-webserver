from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxConfigResponse:
    id: Optional[str] = None
    config_type: Optional[int] = None
    config_name: Optional[str] = None
    config_content: Optional[str] = None
    config_hash: Optional[str] = None
    is_active: Optional[bool] = None
    version_no: Optional[int] = None
    deployed_at: Optional[str] = None
    status: Optional[int] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
