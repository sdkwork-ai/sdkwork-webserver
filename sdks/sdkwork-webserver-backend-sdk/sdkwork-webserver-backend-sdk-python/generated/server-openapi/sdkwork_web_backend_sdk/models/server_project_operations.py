from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_project_operation import ServerProjectOperation


@dataclass
class ServerProjectOperations:
    node_id: str
    path: str
    project_type: str
    operations: List[ServerProjectOperation]
