package types


type DomainDeploymentResponse struct {
	Id string `json:"id"`
	Status int `json:"status"`
	Environment string `json:"environment"`
	VersionTag string `json:"versionTag"`
	CompletedAt string `json:"completedAt"`
	CreatedAt string `json:"createdAt"`
}
