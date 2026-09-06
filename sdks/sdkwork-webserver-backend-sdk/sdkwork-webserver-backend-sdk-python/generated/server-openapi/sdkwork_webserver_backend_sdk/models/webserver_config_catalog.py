from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .webserver_config_entry import WebserverConfigEntry


@dataclass
class WebserverConfigCatalog:
    config_root: str
    items: List[WebserverConfigEntry]
