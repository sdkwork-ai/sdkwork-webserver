from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_files_node import ServerFilesNode


@dataclass
class ServerFilesNodesListResponse:
    code: int
    data: Any
    trace_id: str
