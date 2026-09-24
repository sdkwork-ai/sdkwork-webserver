package types

// The dashboard's cardinal metrics, each reported across the same four **half-open** windows (`today`, `last_7_days`, `current_month`, `lifetime`). Three groups share the window vocabulary but not its meaning: for the entity counts a window names when the subjects arrived, and `lifetime` is therefore the live population rather than an arrival count; for the metered traffic every window is a volume accumulated inside it, and `lifetime` is everything the facts cover — which is why `trafficSince` reports the day they start on; for the storage figures a narrow window is the part of the current holding added inside it, and `lifetime` is what is held *now* — a held figure rather than a running total, since an object removed from storage stops counting in every window including the one it was born in.
`series` reports the same entity metrics at **per-day** granularity, over the window named by `seriesWindow` rather than over the four above: the four card windows belong to the product and are resolved server-side from the clock, while the series window belongs to the caller and comes from the query string, because the plot it feeds also draws the traffic reading's per-day series and that plot has one x domain. A caller that draws both must pass one pair of bounds to both operations rather than omitting them: each operation resolves an omitted bound from the clock on its own, and two defaults are two windows. The two are reported separately so a surface cannot label a day's figure with a period it was not cut against.
type MetricsSummaryResponse struct {
	AsOf string `json:"asOf"`
	PlatformScope bool `json:"platformScope"`
	TrafficSince string `json:"trafficSince"`
	Windows []MetricsWindowBounds `json:"windows"`
	Entities []MetricsMetricTotals `json:"entities"`
	Traffic []MetricsMetricTotals `json:"traffic"`
	Storage []MetricsMetricTotals `json:"storage"`
	Series []MetricsSeries `json:"series"`
	SeriesWindow MetricsSeriesWindow `json:"seriesWindow"`
	UnassembledMetrics []string `json:"unassembledMetrics"`
}
