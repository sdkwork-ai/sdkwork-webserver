package types


type CreateListenerCertificateBindingRequest struct {
	CertificateId string `json:"certificateId"`
	CertificateVersionId string `json:"certificateVersionId"`
	Priority int `json:"priority"`
	IsDefault bool `json:"isDefault"`
}
