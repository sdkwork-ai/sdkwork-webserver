package types


type IssueCertificateRequest struct {
	DomainIds []string `json:"domainIds"`
	CertType int `json:"certType"`
	KeyAlgorithm string `json:"keyAlgorithm"`
	AutoRenew bool `json:"autoRenew"`
	CertName string `json:"certName"`
	CertificateScope string `json:"certificateScope"`
	ValidationMethod string `json:"validationMethod"`
	RenewBeforeDays int `json:"renewBeforeDays"`
	CaProfile string `json:"caProfile"`
	ProviderAccountId string `json:"providerAccountId"`
}
