from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .webserver_config_write_result import WebserverConfigWriteResult


@dataclass
class WebserverConfigsUpdateResponse:
    code: int
    data: Any
    trace_id: str
