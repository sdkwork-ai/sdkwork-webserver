package types


type MetricsWindowValue struct {
	Window string `json:"window"`
	Quantity Int64String `json:"quantity"`
	Unit string `json:"unit"`
}
