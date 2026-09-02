package types


type ServerOperationResult struct {
	OperationId string `json:"operationId"`
	ExitCode int `json:"exitCode"`
	Stdout string `json:"stdout"`
	Stderr string `json:"stderr"`
}
