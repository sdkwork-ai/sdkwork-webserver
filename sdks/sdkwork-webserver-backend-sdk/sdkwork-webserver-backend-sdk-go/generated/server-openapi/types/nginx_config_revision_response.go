package types


type NginxConfigRevisionResponse struct {
	VersionNo int `json:"versionNo"`
	ConfigName string `json:"configName"`
	ConfigContent string `json:"configContent"`
	ConfigHash string `json:"configHash"`
	CreatedBy string `json:"createdBy"`
	CreatedAt string `json:"createdAt"`
}
