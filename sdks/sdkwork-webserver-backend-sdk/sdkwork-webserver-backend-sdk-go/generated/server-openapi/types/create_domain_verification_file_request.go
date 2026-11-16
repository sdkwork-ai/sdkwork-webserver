package types


type CreateDomainVerificationFileRequest struct {
	Platform string `json:"platform"`
	FileName string `json:"fileName"`
	Content string `json:"content"`
}
