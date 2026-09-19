package types


type CreateClusterRequest struct {
	Name string `json:"name"`
	Code string `json:"code"`
	Description string `json:"description"`
	HeartbeatIntervalSeconds int `json:"heartbeatIntervalSeconds"`
	OfflineThresholdSeconds int `json:"offlineThresholdSeconds"`
}
