from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_source_version_response import ApplicationSourceVersionResponse


@dataclass
class ApplicationsSourceVersionsRetrieveResponse:
    code: int
    data: Any
    trace_id: str
