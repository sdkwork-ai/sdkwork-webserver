package types

// Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently.
type TrafficUsageStatisticsResponse struct {
	DateFrom string `json:"dateFrom"`
	DateTo string `json:"dateTo"`
	PlatformScope bool `json:"platformScope"`
	Totals []TrafficUsageTotal `json:"totals"`
	Daily []TrafficUsageDailyPoint `json:"daily"`
	Apps []TrafficUsageAppTotal `json:"apps"`
	Tenants []TrafficUsageTenantTotal `json:"tenants"`
}
