package types


type CreateDomainRequest struct {
	Hostname string `json:"hostname"`
	IsPrimary bool `json:"isPrimary"`
	SslEnabled bool `json:"sslEnabled"`
	SslProvider string `json:"sslProvider"`
}
