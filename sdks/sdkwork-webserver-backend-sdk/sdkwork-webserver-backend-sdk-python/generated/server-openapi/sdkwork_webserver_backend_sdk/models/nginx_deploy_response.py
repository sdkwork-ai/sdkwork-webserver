from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class NginxDeployResponse:
    success: Optional[bool] = None
    config_id: Optional[str] = None
    deployed_at: Optional[str] = None
    reload_result: Optional[Dict[str, Any]] = None
