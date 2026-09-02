package types


type DomainVerifyResponse struct {
	Verified bool `json:"verified"`
	Status string `json:"status"`
	Method string `json:"method"`
	RecordName string `json:"recordName"`
	RecordValue string `json:"recordValue"`
	AttemptCount int `json:"attemptCount"`
	ExpiresAt string `json:"expiresAt"`
	NextAttemptAt string `json:"nextAttemptAt"`
	CheckedAt string `json:"checkedAt"`
	FailureCode string `json:"failureCode"`
}
