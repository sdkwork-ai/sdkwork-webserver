from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .listener_certificate_binding_response import ListenerCertificateBindingResponse
    from .page_info import PageInfo


@dataclass
class ApplicationsDomainsListenerCertificateBindingsListResponse:
    code: int
    data: Any
    trace_id: str
