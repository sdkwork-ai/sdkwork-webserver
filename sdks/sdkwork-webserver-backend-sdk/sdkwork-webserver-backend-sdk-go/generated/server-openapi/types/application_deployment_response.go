package types


type ApplicationDeploymentResponse struct {
	Id string `json:"id"`
	SiteId string `json:"siteId"`
	SourceVersionId string `json:"sourceVersionId"`
	Status int `json:"status"`
	DeployType int `json:"deployType"`
	Environment string `json:"environment"`
	VersionTag string `json:"versionTag"`
	CommitHash string `json:"commitHash"`
	SourceRef string `json:"sourceRef"`
	RollbackFromDeploymentId string `json:"rollbackFromDeploymentId"`
	ArtifactDriveUri string `json:"artifactDriveUri"`
	ArtifactSize string `json:"artifactSize"`
	ArtifactHash string `json:"artifactHash"`
	StartedAt string `json:"startedAt"`
	CompletedAt string `json:"completedAt"`
	DurationMs string `json:"durationMs"`
	CreatedAt string `json:"createdAt"`
}
