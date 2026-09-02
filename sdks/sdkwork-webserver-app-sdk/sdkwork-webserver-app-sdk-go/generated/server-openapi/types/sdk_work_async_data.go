package types


type SdkWorkAsyncData struct {
	Accepted bool `json:"accepted"`
	OperationId string `json:"operationId"`
	Status string `json:"status"`
	PollUrl string `json:"pollUrl"`
}
