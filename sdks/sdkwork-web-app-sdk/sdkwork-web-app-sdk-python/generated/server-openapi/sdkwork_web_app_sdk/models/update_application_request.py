from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_store_listing import ApplicationStoreListing


@dataclass
class UpdateApplicationRequest:
    name: Optional[str] = None
    description: Optional[str] = None
    runtime_config: Optional[Dict[str, Any]] = None
    store_listing: Optional[ApplicationStoreListing] = None
