from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .nginx_deploy_response import NginxDeployResponse


@dataclass
class ConfigsDeployResponse:
    code: int
    data: Any
    trace_id: str
