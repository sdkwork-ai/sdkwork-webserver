package types


type NginxDeployResponse struct {
	Success bool `json:"success"`
	ConfigId string `json:"configId"`
	DeployedAt string `json:"deployedAt"`
	ReloadResult map[string]interface{} `json:"reloadResult"`
}
