from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_store_listing import ApplicationStoreListing


@dataclass
class ApplicationResponse:
    id: str
    name: str
    slug: str
    site_type: int
    status: int
    created_at: str
    updated_at: str
    description: Optional[str] = None
    app_kind: Optional[str] = None
    runtime_config: Optional[Dict[str, Any]] = None
    store_listing: Optional[ApplicationStoreListing] = None
