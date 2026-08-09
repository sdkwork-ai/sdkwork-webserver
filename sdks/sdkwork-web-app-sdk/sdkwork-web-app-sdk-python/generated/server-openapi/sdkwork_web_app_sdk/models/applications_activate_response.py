from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_response import ApplicationResponse


@dataclass
class ApplicationsActivateResponse:
    code: int
    data: Any
    trace_id: str
