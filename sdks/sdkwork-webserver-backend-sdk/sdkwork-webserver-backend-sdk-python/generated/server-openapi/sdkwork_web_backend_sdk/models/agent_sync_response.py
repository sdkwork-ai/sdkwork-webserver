from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .agent_certificate_bundle import AgentCertificateBundle
    from .agent_nginx_config_bundle import AgentNginxConfigBundle


@dataclass
class AgentSyncResponse:
    server_id: str
    sync_version: str
    unchanged: bool
    nginx_configs: List[AgentNginxConfigBundle]
    certificates: List[AgentCertificateBundle]
