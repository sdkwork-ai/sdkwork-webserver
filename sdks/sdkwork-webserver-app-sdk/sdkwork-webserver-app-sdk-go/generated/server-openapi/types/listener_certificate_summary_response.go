package types


type ListenerCertificateSummaryResponse struct {
	CertName string `json:"certName"`
	Identifiers []CertificateIdentifierResponse `json:"identifiers"`
	Issuer string `json:"issuer"`
	Fingerprint string `json:"fingerprint"`
	NotAfter string `json:"notAfter"`
	Status string `json:"status"`
}
