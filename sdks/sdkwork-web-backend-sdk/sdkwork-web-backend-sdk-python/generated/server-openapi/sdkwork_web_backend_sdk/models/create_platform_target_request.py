from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreatePlatformTargetRequest:
    target_key: str
    platform: str
    tech_stack: Optional[str] = None
    architectures: Optional[List[str]] = None
    bundle_id: Optional[str] = None
    package_name: Optional[str] = None
    app_id: Optional[str] = None
    bundle_name: Optional[str] = None
    allowed_channels: Optional[List[str]] = None
