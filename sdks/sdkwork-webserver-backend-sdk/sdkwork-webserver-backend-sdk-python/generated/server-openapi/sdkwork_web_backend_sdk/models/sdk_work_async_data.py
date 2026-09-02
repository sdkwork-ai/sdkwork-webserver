from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any, Literal


@dataclass
class SdkWorkAsyncData:
    accepted: Literal[True]
    operation_id: str
    status: str
    poll_url: Optional[str] = None
