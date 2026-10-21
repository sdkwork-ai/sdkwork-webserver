package types


type NginxConfigRevisionSummary struct {
	VersionNo int `json:"versionNo"`
	ConfigName string `json:"configName"`
	ConfigHash string `json:"configHash"`
	CreatedBy string `json:"createdBy"`
	CreatedAt string `json:"createdAt"`
}
