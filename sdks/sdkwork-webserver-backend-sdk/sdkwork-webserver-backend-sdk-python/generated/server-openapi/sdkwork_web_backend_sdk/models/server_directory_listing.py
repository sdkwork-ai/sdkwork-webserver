from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_entry import ServerEntry


@dataclass
class ServerDirectoryListing:
    node_id: str
    path: str
    parent_path: Optional[str]
    entries: List[ServerEntry]
