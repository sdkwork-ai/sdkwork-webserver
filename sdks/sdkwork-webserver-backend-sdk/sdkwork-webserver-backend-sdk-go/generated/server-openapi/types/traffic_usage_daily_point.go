package types


type TrafficUsageDailyPoint struct {
	UsageDate string `json:"usageDate"`
	Dimension string `json:"dimension"`
	Quantity Int64String `json:"quantity"`
}
