package types


type ApplicationDomainResponse struct {
	Id string `json:"id"`
	Hostname string `json:"hostname"`
	RootDomainId string `json:"rootDomainId"`
	RecordName string `json:"recordName"`
	ApplicationId string `json:"applicationId"`
	ApplicationName string `json:"applicationName"`
	CertificateCount string `json:"certificateCount"`
	IsPrimary bool `json:"isPrimary"`
	IsVerified bool `json:"isVerified"`
	SslEnabled bool `json:"sslEnabled"`
	SslProvider string `json:"sslProvider"`
	Status int `json:"status"`
	LatestDeployment DomainDeploymentResponse `json:"latestDeployment"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
