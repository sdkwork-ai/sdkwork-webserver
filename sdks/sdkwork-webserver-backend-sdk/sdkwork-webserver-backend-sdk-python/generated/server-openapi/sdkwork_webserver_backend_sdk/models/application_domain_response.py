from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .domain_deployment_response import DomainDeploymentResponse


@dataclass
class ApplicationDomainResponse:
    id: str
    hostname: str
    certificate_count: str
    is_primary: bool
    is_verified: bool
    ssl_enabled: bool
    status: int
    created_at: str
    root_domain_id: Optional[str] = None
    record_name: Optional[str] = None
    application_id: Optional[str] = None
    application_name: Optional[str] = None
    ssl_provider: Optional[str] = None
    latest_deployment: Optional[DomainDeploymentResponse] = None
    updated_at: Optional[str] = None
