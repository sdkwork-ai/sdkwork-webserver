from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .sdk_work_command_data import SdkWorkCommandData


@dataclass
class SdkWorkCommandResponse:
    code: int
    data: Any
    trace_id: str
