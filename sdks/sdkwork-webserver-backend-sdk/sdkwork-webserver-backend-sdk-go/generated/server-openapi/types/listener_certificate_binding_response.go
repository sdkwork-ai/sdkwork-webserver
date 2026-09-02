package types


type ListenerCertificateBindingResponse struct {
	Id string `json:"id"`
	SiteId string `json:"siteId"`
	DomainId string `json:"domainId"`
	CertificateId string `json:"certificateId"`
	DesiredCertificateVersionId string `json:"desiredCertificateVersionId"`
	CurrentCertificateVersionId string `json:"currentCertificateVersionId"`
	DesiredCertificate ListenerCertificateSummaryResponse `json:"desiredCertificate"`
	CurrentCertificate ListenerCertificateSummaryResponse `json:"currentCertificate"`
	KeyAlgorithm string `json:"keyAlgorithm"`
	Priority int `json:"priority"`
	IsDefault bool `json:"isDefault"`
	Status string `json:"status"`
	ActivatedAt string `json:"activatedAt"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
