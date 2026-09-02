package types


type CreateNginxConfigRequest struct {
	ConfigType int `json:"configType"`
	ConfigName string `json:"configName"`
	ConfigContent string `json:"configContent"`
	SiteId string `json:"siteId"`
}
