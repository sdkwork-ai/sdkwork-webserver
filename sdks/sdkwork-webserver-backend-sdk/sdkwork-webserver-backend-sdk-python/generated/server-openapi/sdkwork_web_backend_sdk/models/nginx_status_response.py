from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxStatusResponse:
    running: Optional[bool] = None
    version: Optional[str] = None
    pid: Optional[int] = None
    active_connections: Optional[int] = None
    config_path: Optional[str] = None
    uptime: Optional[str] = None
