package types


type SourceVersionResponse struct {
	Id string `json:"id"`
	ApplicationId string `json:"applicationId"`
	VersionTag string `json:"versionTag"`
	SourceType string `json:"sourceType"`
	SourceRef string `json:"sourceRef"`
	CommitHash string `json:"commitHash"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
	ConfigSnapshot SourceVersionConfigSnapshot `json:"configSnapshot"`
	Status int `json:"status"`
	Retained bool `json:"retained"`
	CreatedAt string `json:"createdAt"`
}
