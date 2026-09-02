package types


type AgentCertificateBundle struct {
	CertificateId string `json:"certificateId"`
	CertName string `json:"certName"`
	Fingerprint string `json:"fingerprint"`
	Hostnames []string `json:"hostnames"`
	FullchainPem string `json:"fullchainPem"`
	PrivkeyPem string `json:"privkeyPem"`
}
