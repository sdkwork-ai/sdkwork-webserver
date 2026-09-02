package types


type CertificateIdentifierResponse struct {
	DomainId string `json:"domainId"`
	Hostname string `json:"hostname"`
	IdentifierType string `json:"identifierType"`
	Position int `json:"position"`
}
