package types


type CreateServerResponse struct {
	Id string `json:"id"`
	Name string `json:"name"`
	Host string `json:"host"`
	TenantScopeHash string `json:"tenantScopeHash"`
	SshPort int `json:"sshPort"`
	Status int `json:"status"`
	LastHeartbeatAt string `json:"lastHeartbeatAt"`
	CreatedAt string `json:"createdAt"`
	AgentToken string `json:"agentToken"`
}
