from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_project_operations import ServerProjectOperations


@dataclass
class ServerFilesNodeOperationsListResponse:
    code: int
    data: Any
    trace_id: str
