package types


type DomainResponse struct {
	Id string `json:"id"`
	Hostname string `json:"hostname"`
	ApplicationId string `json:"applicationId"`
	ApplicationName string `json:"applicationName"`
	CertificateCount string `json:"certificateCount"`
	IsPrimary bool `json:"isPrimary"`
	IsVerified bool `json:"isVerified"`
	SslEnabled bool `json:"sslEnabled"`
	SslProvider string `json:"sslProvider"`
	Status int `json:"status"`
	CreatedAt string `json:"createdAt"`
}
