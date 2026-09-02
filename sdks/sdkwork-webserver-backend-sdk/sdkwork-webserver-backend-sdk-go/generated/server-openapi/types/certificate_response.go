package types


type CertificateResponse struct {
	Id string `json:"id"`
	CertName string `json:"certName"`
	Identifiers []CertificateIdentifierResponse `json:"identifiers"`
	CertType int `json:"certType"`
	Issuer string `json:"issuer"`
	Fingerprint string `json:"fingerprint"`
	KeyAlgorithm string `json:"keyAlgorithm"`
	NotBefore string `json:"notBefore"`
	NotAfter string `json:"notAfter"`
	AutoRenew bool `json:"autoRenew"`
	RenewalStatus string `json:"renewalStatus"`
	Status string `json:"status"`
	CreatedAt string `json:"createdAt"`
}
