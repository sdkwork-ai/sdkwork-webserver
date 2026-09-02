from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .server_file_content import ServerFileContent


@dataclass
class ServerFilesNodeReadResponse:
    code: int
    data: Any
    trace_id: str
