package types


type DomainWechatVerificationCheckResponse struct {
	Reachable bool `json:"reachable"`
	Matched bool `json:"matched"`
	StatusCode int `json:"statusCode"`
	Scheme string `json:"scheme"`
	Detail string `json:"detail"`
	CheckedAt string `json:"checkedAt"`
}
