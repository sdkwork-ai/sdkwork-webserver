from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class TrafficUsageDailyPoint:
    usage_date: str
    dimension: str
    quantity: str
