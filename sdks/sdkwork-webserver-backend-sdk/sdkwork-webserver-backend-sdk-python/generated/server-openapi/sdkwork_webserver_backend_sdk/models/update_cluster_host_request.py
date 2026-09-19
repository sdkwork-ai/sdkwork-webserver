from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateClusterHostRequest:
    name: Optional[str] = None
    cluster_id: Optional[str] = None
