from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class SourceVersionConfigSnapshot:
    app_config_path: str
    deployment_config_path: str
    app_config_detected: bool
    deployment_config_detected: bool
