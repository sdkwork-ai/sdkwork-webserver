from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_store_listing import ApplicationStoreListing


@dataclass
class CreateApplicationRequest:
    name: str
    site_type: int
    slug: Optional[str] = None
    description: Optional[str] = None
    application_type: Optional[str] = None
    runtime_config: Optional[Dict[str, Any]] = None
    store_listing: Optional[ApplicationStoreListing] = None
