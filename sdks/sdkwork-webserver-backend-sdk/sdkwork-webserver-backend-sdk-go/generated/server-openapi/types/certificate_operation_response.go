package types


type CertificateOperationResponse struct {
	Id string `json:"id"`
	CertificateId string `json:"certificateId"`
	OperationType string `json:"operationType"`
	Status string `json:"status"`
	AttemptCount int `json:"attemptCount"`
	MaxAttempts int `json:"maxAttempts"`
	NextAttemptAt string `json:"nextAttemptAt"`
	FailureCode string `json:"failureCode"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
	CompletedAt string `json:"completedAt"`
}
