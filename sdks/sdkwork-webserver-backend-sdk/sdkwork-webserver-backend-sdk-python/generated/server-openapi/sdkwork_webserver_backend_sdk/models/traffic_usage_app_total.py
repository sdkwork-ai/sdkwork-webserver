from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class TrafficUsageAppTotal:
    """One app's aggregate over the window. A row carrying neither `appUuid` nor `appSlug` is the **unattributed** bucket: traffic served for a hostname the edge could not resolve to an app. It is reported rather than dropped so the per-app rows keep summing back to the total; a surface must render it as its own row, or the breakdown appears to lose traffic."""
    dimension: str
    quantity: str
    unit: str
    app_uuid: Optional[str] = None
    app_slug: Optional[str] = None
