from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class AgentNginxConfigBundle:
    config_id: str
    domain: str
    config_content: str
    fingerprint: str
    version: str
