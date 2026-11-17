from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .application_store_listing import ApplicationStoreListing


@dataclass
class CreateApplicationRequest:
    """Create input for a carrier row.

There is no `appKind` member: a carrier row's identity is
`deployApplicationId`, and its application kind is a server-owned default
(`STATIC_WEB`, which derives `siteType = 1` and the site carrier
application type `WEB`). The member was removed rather than made optional
so no caller can re-declare a second application identity on the carrier
row (`MIG-2026-0071` deletion-list item 6).
"""
    name: str
    deploy_application_id: str
    slug: Optional[str] = None
    description: Optional[str] = None
    runtime_config: Optional[Dict[str, Any]] = None
    store_listing: Optional[ApplicationStoreListing] = None
