from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .traffic_usage_app_total import TrafficUsageAppTotal
    from .traffic_usage_daily_point import TrafficUsageDailyPoint
    from .traffic_usage_tenant_total import TrafficUsageTenantTotal
    from .traffic_usage_total import TrafficUsageTotal


@dataclass
class TrafficUsageStatisticsResponse:
    """Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently."""
    date_from: str
    date_to: str
    platform_scope: bool
    totals: List[TrafficUsageTotal]
    daily: List[TrafficUsageDailyPoint]
    apps: List[TrafficUsageAppTotal]
    tenants: List[TrafficUsageTenantTotal]
