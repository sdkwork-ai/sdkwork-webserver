from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ServerFilesNode:
    id: str
    name: str
    host: str
    ssh_port: int
    status: str
    filesystem_root: str
    region: Optional[str] = None
