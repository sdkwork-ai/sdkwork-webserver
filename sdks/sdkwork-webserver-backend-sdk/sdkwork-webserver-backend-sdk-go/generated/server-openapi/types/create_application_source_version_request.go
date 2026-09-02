package types


type CreateApplicationSourceVersionRequest struct {
	VersionTag string `json:"versionTag"`
	SourceType string `json:"sourceType"`
	SourceRef string `json:"sourceRef"`
	CommitHash string `json:"commitHash"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
	ConfigSnapshot ApplicationSourceVersionConfigSnapshot `json:"configSnapshot"`
}
