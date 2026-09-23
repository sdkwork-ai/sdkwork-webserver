from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class TrafficUsageTenantTotal:
    tenant_id: str
    dimension: str
    quantity: str
    unit: str
