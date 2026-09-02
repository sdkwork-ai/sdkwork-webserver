package types


type AgentHeartbeatRequest struct {
	AgentVersion string `json:"agentVersion"`
	NginxEnabled bool `json:"nginxEnabled"`
	ActiveConfigs string `json:"activeConfigs"`
	LastSyncVersion string `json:"lastSyncVersion"`
	CertificateObservations []AgentCertificateObservation `json:"certificateObservations"`
}
