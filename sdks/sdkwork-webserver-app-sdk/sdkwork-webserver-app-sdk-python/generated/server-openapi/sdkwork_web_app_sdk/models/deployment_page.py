from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .deployment_response import DeploymentResponse


@dataclass
class DeploymentPage:
    items: Optional[List[DeploymentResponse]] = None
    total: Optional[str] = None
