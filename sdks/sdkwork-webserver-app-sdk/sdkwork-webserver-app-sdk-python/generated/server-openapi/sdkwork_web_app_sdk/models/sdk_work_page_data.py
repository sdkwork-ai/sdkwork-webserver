from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .page_info import PageInfo


@dataclass
class SdkWorkPageData:
    items: List[Dict[str, Any]]
    page_info: PageInfo
