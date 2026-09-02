package types


type HealthCheckPage struct {
	Items []HealthCheckResponse `json:"items"`
	Total string `json:"total"`
}
