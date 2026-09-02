from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .nginx_config_response import NginxConfigResponse


@dataclass
class NginxConfigPage:
    items: Optional[List[NginxConfigResponse]] = None
    total: Optional[str] = None
