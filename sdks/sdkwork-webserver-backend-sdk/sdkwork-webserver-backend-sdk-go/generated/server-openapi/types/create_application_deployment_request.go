package types

// Deployment source command. Git deployments (deployType 2) require an HTTPS sourceRef and may omit artifact fields. Other deployment types require artifactDriveUri, artifactSize, and artifactHash together.
type CreateApplicationDeploymentRequest struct {
	SourceVersionId string `json:"sourceVersionId"`
	DeployType int `json:"deployType"`
	Environment string `json:"environment"`
	VersionTag string `json:"versionTag"`
	CommitHash string `json:"commitHash"`
	SourceRef string `json:"sourceRef"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
}
