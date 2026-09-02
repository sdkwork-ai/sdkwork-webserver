package types


type UpdateNginxConfigRequest struct {
	ConfigContent string `json:"configContent"`
	ConfigName string `json:"configName"`
}
