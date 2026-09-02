from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateDomainApplicationBindingRequest:
    application_id: str
    is_primary: Optional[bool] = None
