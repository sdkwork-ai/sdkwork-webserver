from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateClusterInstanceRequest:
    name: Optional[str] = None
    status: Optional[int] = None
    public_endpoint: Optional[str] = None
