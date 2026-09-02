from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class RootDomainResponse:
    id: str
    hostname: str
    status: int
    subdomain_count: str
    bound_subdomain_count: str
    verified_subdomain_count: str
    https_subdomain_count: str
    active_deployment_count: str
    created_at: str
    updated_at: str
