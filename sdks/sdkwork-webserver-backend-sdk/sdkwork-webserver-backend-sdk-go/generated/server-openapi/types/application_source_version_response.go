package types


type ApplicationSourceVersionResponse struct {
	Id string `json:"id"`
	SiteId string `json:"siteId"`
	VersionTag string `json:"versionTag"`
	SourceType string `json:"sourceType"`
	SourceRef string `json:"sourceRef"`
	CommitHash string `json:"commitHash"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
	ConfigSnapshot ApplicationSourceVersionConfigSnapshot `json:"configSnapshot"`
	Status int `json:"status"`
	Retained bool `json:"retained"`
	CreatedAt string `json:"createdAt"`
}
