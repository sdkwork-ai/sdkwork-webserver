package types


type DeploymentResponse struct {
	Id string `json:"id"`
	ApplicationId string `json:"applicationId"`
	DeployType int `json:"deployType"`
	SourceVersionId string `json:"sourceVersionId"`
	VersionTag string `json:"versionTag"`
	CommitHash string `json:"commitHash"`
	SourceRef string `json:"sourceRef"`
	RollbackFromDeploymentId string `json:"rollbackFromDeploymentId"`
	Environment string `json:"environment"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
	Status int `json:"status"`
	StartedAt string `json:"startedAt"`
	CompletedAt string `json:"completedAt"`
	DurationMs string `json:"durationMs"`
	CreatedAt string `json:"createdAt"`
}
