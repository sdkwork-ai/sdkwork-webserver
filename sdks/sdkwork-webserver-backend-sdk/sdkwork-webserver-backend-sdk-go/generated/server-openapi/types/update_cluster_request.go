package types


type UpdateClusterRequest struct {
	Name string `json:"name"`
	Description string `json:"description"`
	Status int `json:"status"`
	HeartbeatIntervalSeconds int `json:"heartbeatIntervalSeconds"`
	OfflineThresholdSeconds int `json:"offlineThresholdSeconds"`
	LbStrategy string `json:"lbStrategy"`
	ServedDomains []string `json:"servedDomains"`
}
