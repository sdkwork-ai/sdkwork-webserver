from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .create_server_response import CreateServerResponse


@dataclass
class ServersCreateResponse201:
    code: int
    data: Any
    trace_id: str
