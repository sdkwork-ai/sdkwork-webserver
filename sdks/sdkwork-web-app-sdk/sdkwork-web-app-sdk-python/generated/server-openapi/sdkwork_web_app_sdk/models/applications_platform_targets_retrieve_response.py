from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .platform_target_response import PlatformTargetResponse


@dataclass
class ApplicationsPlatformTargetsRetrieveResponse:
    code: int
    data: Any
    trace_id: str
