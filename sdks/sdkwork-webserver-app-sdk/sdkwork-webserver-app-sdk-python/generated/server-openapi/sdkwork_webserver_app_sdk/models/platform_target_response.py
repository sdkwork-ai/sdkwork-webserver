from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class PlatformTargetResponse:
    id: Optional[str] = None
    app_id: Optional[str] = None
    target_key: Optional[str] = None
    platform: Optional[str] = None
    tech_stack: Optional[str] = None
    architectures: Optional[List[str]] = None
    bundle_id: Optional[str] = None
    package_name: Optional[str] = None
    app_id_value: Optional[str] = None
    bundle_name: Optional[str] = None
    target_status: Optional[str] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
