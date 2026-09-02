package types


type AgentSyncResponse struct {
	ServerId string `json:"serverId"`
	SyncVersion string `json:"syncVersion"`
	Unchanged bool `json:"unchanged"`
	NginxConfigs []AgentNginxConfigBundle `json:"nginxConfigs"`
	Certificates []AgentCertificateBundle `json:"certificates"`
}
