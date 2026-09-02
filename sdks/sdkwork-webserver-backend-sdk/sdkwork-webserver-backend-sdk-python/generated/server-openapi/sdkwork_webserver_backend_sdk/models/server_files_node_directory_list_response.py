from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_directory_listing import ServerDirectoryListing


@dataclass
class ServerFilesNodeDirectoryListResponse:
    code: int
    data: Any
    trace_id: str
