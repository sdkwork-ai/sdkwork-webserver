from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .media_resource import MediaResource


@dataclass
class ApplicationStoreListing:
    icon: Optional[MediaResource] = None
    cover: Optional[MediaResource] = None
    previews: Optional[List[MediaResource]] = None
    short_description: Optional[str] = None
    full_description: Optional[str] = None
    release_notes: Optional[str] = None
    category: Optional[str] = None
    keywords: Optional[List[str]] = None
    support_url: Optional[str] = None
    privacy_policy_url: Optional[str] = None
    official_website_url: Optional[str] = None
