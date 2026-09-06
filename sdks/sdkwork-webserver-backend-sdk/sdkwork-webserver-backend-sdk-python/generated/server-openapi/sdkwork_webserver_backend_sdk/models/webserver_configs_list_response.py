from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .webserver_config_catalog import WebserverConfigCatalog


@dataclass
class WebserverConfigsListResponse:
    code: int
    data: Any
    trace_id: str
