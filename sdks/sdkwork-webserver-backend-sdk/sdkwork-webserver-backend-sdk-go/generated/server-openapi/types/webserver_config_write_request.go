package types


type WebserverConfigWriteRequest struct {
	Content string `json:"content"`
	ExpectedSha256 string `json:"expectedSha256"`
}
