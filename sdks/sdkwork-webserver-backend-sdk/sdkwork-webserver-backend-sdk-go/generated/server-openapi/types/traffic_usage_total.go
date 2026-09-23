package types


type TrafficUsageTotal struct {
	Dimension string `json:"dimension"`
	Quantity Int64String `json:"quantity"`
	Unit string `json:"unit"`
}
