package types


type AgentHeartbeatResponse struct {
	ServerId string `json:"serverId"`
	Status int `json:"status"`
	AcknowledgedAt string `json:"acknowledgedAt"`
}
