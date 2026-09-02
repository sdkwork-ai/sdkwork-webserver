from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any, Literal


@dataclass
class SdkWorkCommandData:
    accepted: Literal[True]
    resource_id: Optional[str] = None
    status: Optional[str] = None
