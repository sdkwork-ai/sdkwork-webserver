from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .nginx_config_response import NginxConfigResponse
    from .page_info import PageInfo


@dataclass
class ConfigsListResponse:
    code: int
    data: Any
    trace_id: str
