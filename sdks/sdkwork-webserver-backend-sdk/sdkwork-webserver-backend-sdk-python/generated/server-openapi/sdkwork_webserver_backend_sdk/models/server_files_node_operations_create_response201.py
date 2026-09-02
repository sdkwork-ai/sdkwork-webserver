from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_operation_result import ServerOperationResult


@dataclass
class ServerFilesNodeOperationsCreateResponse201:
    code: int
    data: Any
    trace_id: str
