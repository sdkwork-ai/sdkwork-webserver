from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_store_listing import ApplicationStoreListing


@dataclass
class ApplicationResponse:
    id: Optional[str] = None
    name: Optional[str] = None
    slug: Optional[str] = None
    description: Optional[str] = None
    site_id: Optional[str] = None
    app_kind: Optional[str] = None
    site_type: Optional[int] = None
    status: Optional[int] = None
    runtime_config: Optional[Dict[str, Any]] = None
    store_listing: Optional[ApplicationStoreListing] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
