from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ServerFileContent:
    node_id: str
    path: str
    content: str
    size: str
