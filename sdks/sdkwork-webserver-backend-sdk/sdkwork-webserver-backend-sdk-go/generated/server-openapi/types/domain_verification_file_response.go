package types


type DomainVerificationFileResponse struct {
	Id string `json:"id"`
	RootDomainId string `json:"rootDomainId"`
	Platform string `json:"platform"`
	FileName string `json:"fileName"`
	Content string `json:"content"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
