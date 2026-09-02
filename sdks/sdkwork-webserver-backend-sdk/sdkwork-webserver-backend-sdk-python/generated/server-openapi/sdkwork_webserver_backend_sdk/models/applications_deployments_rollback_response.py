from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_deployment_response import ApplicationDeploymentResponse


@dataclass
class ApplicationsDeploymentsRollbackResponse:
    code: int
    data: Any
    trace_id: str
