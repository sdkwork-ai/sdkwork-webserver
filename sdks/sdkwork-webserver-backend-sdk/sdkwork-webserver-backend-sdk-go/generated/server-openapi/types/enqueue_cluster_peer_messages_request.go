package types


type EnqueueClusterPeerMessagesRequest struct {
	ClusterId string `json:"clusterId"`
	ToInstanceId string `json:"toInstanceId"`
	FromInstanceId string `json:"fromInstanceId"`
	MessageType string `json:"messageType"`
	Payload map[string]interface{} `json:"payload"`
	ExpiresInSeconds int `json:"expiresInSeconds"`
}
