package types


type RootDomainResponse struct {
	Id string `json:"id"`
	Hostname string `json:"hostname"`
	Status int `json:"status"`
	SubdomainCount Int64String `json:"subdomainCount"`
	BoundSubdomainCount Int64String `json:"boundSubdomainCount"`
	VerifiedSubdomainCount Int64String `json:"verifiedSubdomainCount"`
	HttpsSubdomainCount Int64String `json:"httpsSubdomainCount"`
	ActiveDeploymentCount Int64String `json:"activeDeploymentCount"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
