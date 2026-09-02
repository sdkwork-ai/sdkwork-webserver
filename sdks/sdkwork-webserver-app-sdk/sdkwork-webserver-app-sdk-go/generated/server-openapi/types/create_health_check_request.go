package types


type CreateHealthCheckRequest struct {
	CheckType int `json:"checkType"`
	CheckUrl string `json:"checkUrl"`
	CheckInterval int `json:"checkInterval"`
	TimeoutMs int `json:"timeoutMs"`
	RetryCount int `json:"retryCount"`
}
