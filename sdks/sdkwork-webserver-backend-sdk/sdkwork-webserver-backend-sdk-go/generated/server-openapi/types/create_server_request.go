package types


type CreateServerRequest struct {
	Name string `json:"name"`
	Host string `json:"host"`
	TenantScopeHash string `json:"tenantScopeHash"`
	SshPort int `json:"sshPort"`
}
