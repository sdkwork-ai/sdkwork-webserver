package types


type ClusterHeartbeatSampleResponse struct {
	Id string `json:"id"`
	Status int `json:"status"`
	LatencyMs int `json:"latencyMs"`
	Metrics map[string]interface{} `json:"metrics"`
	ReportedAt string `json:"reportedAt"`
}
