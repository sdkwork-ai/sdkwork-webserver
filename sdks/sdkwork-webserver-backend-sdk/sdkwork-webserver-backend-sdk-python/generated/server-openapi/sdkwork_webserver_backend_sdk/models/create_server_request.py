from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateServerRequest:
    name: str
    host: str
    tenant_scope_hash: str
    ssh_port: int
