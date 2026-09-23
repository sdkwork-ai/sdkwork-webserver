package types


type ClusterProbeRunResponse struct {
	Healthy bool `json:"healthy"`
	LatencyMs int `json:"latencyMs"`
	Failures int `json:"failures"`
	Ejected bool `json:"ejected"`
	Recovered bool `json:"recovered"`
}
