from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateNginxConfigRequest:
    config_type: int
    config_name: str
    config_content: str
    site_id: str
