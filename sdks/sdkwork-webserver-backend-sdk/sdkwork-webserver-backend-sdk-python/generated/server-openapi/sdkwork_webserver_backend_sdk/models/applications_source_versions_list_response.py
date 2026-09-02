from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_source_version_response import ApplicationSourceVersionResponse
    from .page_info import PageInfo


@dataclass
class ApplicationsSourceVersionsListResponse:
    code: int
    data: Any
    trace_id: str
