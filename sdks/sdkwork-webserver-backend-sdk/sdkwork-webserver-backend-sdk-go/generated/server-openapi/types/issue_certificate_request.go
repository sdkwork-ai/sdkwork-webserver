package types


type IssueCertificateRequest struct {
	DomainIds []string `json:"domainIds"`
	CertType int `json:"certType"`
	KeyAlgorithm string `json:"keyAlgorithm"`
	AutoRenew bool `json:"autoRenew"`
}
