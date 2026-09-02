package types


type CertificateDistributionResponse struct {
	ServerId string `json:"serverId"`
	ServerName string `json:"serverName"`
	Host string `json:"host"`
	DesiredSyncVersion string `json:"desiredSyncVersion"`
	AppliedSyncVersion string `json:"appliedSyncVersion"`
	Status string `json:"status"`
	LastHeartbeatAt string `json:"lastHeartbeatAt"`
}
