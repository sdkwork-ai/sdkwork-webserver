package types


type HealthCheckResponse struct {
	Id string `json:"id"`
	CheckType int `json:"checkType"`
	CheckUrl string `json:"checkUrl"`
	CheckInterval int `json:"checkInterval"`
	TimeoutMs int `json:"timeoutMs"`
	RetryCount int `json:"retryCount"`
	Status int `json:"status"`
	CreatedAt string `json:"createdAt"`
}
