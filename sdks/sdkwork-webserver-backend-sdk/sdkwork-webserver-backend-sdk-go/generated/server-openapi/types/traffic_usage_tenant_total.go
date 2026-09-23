package types


type TrafficUsageTenantTotal struct {
	TenantId Int64String `json:"tenantId"`
	Dimension string `json:"dimension"`
	Quantity Int64String `json:"quantity"`
	Unit string `json:"unit"`
}
