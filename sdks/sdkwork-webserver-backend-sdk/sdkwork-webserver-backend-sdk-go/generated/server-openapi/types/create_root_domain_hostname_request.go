package types


type CreateRootDomainHostnameRequest struct {
	RecordName string `json:"recordName"`
	ApplicationId string `json:"applicationId"`
	IsPrimary bool `json:"isPrimary"`
	SslEnabled bool `json:"sslEnabled"`
	SslProvider string `json:"sslProvider"`
}
