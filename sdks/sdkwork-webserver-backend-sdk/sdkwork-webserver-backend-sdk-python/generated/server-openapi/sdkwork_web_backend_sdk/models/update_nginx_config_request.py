from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateNginxConfigRequest:
    config_content: Optional[str] = None
    config_name: Optional[str] = None
