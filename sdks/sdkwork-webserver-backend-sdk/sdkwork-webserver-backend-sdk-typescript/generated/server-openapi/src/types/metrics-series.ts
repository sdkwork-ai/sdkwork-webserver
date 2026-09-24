import type { MetricsSeriesPoint } from './metrics-series-point';

/** One entity metric's per-day arrivals over `MetricsSeriesWindow`, days ascending. Points are **sparse**: a day the metric gained nothing may carry no point at all, because a `GROUP BY` cannot report a day it never saw. A day inside the window with no point is a real `0`; emitting an explicit zero for every day would make the response's size a function of the window rather than of the data. */
export interface MetricsSeries {
  metric: string;
  unit: string;
  points: MetricsSeriesPoint[];
}
