package types


type CreateManagedDomainRequest struct {
	Hostname string `json:"hostname"`
	ApplicationId string `json:"applicationId"`
	IsPrimary bool `json:"isPrimary"`
	SslEnabled bool `json:"sslEnabled"`
	SslProvider string `json:"sslProvider"`
}
