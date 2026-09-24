from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .metrics_series_point import MetricsSeriesPoint


@dataclass
class MetricsSeries:
    """One entity metric's per-day arrivals over `MetricsSeriesWindow`, days ascending. Points are **sparse**: a day the metric gained nothing may carry no point at all, because a `GROUP BY` cannot report a day it never saw. A day inside the window with no point is a real `0`; emitting an explicit zero for every day would make the response's size a function of the window rather than of the data."""
    metric: str
    unit: str
    points: List[MetricsSeriesPoint]
