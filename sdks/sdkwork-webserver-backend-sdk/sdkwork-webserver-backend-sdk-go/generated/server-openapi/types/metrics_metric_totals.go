package types

// One metric across every window. `metric` is one of the entity ids (`users`, `tenants`, `applications`, `agents`), one of the storage ids (`storage.used_bytes`, `storage.object_count`), or a metered dimension (`traffic.requests`, …), which is left open on the same terms as the traffic readings: a dimension this contract has not heard of reaches the surface instead of failing the response.
type MetricsMetricTotals struct {
	Metric string `json:"metric"`
	Unit string `json:"unit"`
	Values []MetricsWindowValue `json:"values"`
}
