package types


type NginxConfigResponse struct {
	Id string `json:"id"`
	ConfigType int `json:"configType"`
	ConfigName string `json:"configName"`
	ConfigContent string `json:"configContent"`
	ConfigHash string `json:"configHash"`
	IsActive bool `json:"isActive"`
	VersionNo int `json:"versionNo"`
	DeployedAt string `json:"deployedAt"`
	Status int `json:"status"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
