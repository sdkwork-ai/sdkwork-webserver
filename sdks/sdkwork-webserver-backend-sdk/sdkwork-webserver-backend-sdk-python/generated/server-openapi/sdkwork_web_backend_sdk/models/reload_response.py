from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .nginx_reload_response import NginxReloadResponse


@dataclass
class ReloadResponse:
    code: int
    data: Any
    trace_id: str
