from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdateRootDomainRequest:
    """Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable."""
    display_name: Optional[str] = None
    dns_provider: Optional[str] = None
    provider_zone_ref: Optional[str] = None
    status: Optional[int] = None
