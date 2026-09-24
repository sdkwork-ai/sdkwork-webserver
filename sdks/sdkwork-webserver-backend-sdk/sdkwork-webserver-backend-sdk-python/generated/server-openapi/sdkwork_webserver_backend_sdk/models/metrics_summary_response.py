from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .metrics_metric_totals import MetricsMetricTotals
    from .metrics_series import MetricsSeries
    from .metrics_series_window import MetricsSeriesWindow
    from .metrics_window_bounds import MetricsWindowBounds


@dataclass
class MetricsSummaryResponse:
    """The dashboard's cardinal metrics, each reported across the same four **half-open** windows (`today`, `last_7_days`, `current_month`, `lifetime`). Three groups share the window vocabulary but not its meaning: for the entity counts a window names when the subjects arrived, and `lifetime` is therefore the live population rather than an arrival count; for the metered traffic every window is a volume accumulated inside it, and `lifetime` is everything the facts cover — which is why `trafficSince` reports the day they start on; for the storage figures a narrow window is the part of the current holding added inside it, and `lifetime` is what is held *now* — a held figure rather than a running total, since an object removed from storage stops counting in every window including the one it was born in.
`series` reports the same entity metrics at **per-day** granularity, over the window named by `seriesWindow` rather than over the four above: the four card windows belong to the product and are resolved server-side from the clock, while the series window belongs to the caller and comes from the query string, because the plot it feeds also draws the traffic reading's per-day series and that plot has one x domain. A caller that draws both must pass one pair of bounds to both operations rather than omitting them: each operation resolves an omitted bound from the clock on its own, and two defaults are two windows. The two are reported separately so a surface cannot label a day's figure with a period it was not cut against."""
    as_of: str
    platform_scope: bool
    windows: List[MetricsWindowBounds]
    entities: List[MetricsMetricTotals]
    traffic: List[MetricsMetricTotals]
    storage: List[MetricsMetricTotals]
    series: List[MetricsSeries]
    series_window: MetricsSeriesWindow
    unassembled_metrics: List[str]
    traffic_since: Optional[str] = None
