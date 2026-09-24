package types


type MetricsSeriesPoint struct {
	Date string `json:"date"`
	Quantity Int64String `json:"quantity"`
}
