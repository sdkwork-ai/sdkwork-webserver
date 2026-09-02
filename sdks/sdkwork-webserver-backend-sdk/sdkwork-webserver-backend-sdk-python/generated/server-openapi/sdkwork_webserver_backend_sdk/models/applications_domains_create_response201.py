from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_domain_response import ApplicationDomainResponse


@dataclass
class ApplicationsDomainsCreateResponse201:
    code: int
    data: Any
    trace_id: str
