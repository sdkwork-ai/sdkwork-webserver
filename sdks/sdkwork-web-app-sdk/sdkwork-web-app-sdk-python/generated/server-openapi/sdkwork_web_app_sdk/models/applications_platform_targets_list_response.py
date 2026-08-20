from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .page_info import PageInfo
    from .platform_target_response import PlatformTargetResponse


@dataclass
class ApplicationsPlatformTargetsListResponse:
    code: int
    data: Any
    trace_id: str
