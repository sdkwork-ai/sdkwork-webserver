package types


type PublishClusterSyncRequest struct {
	Kind string `json:"kind"`
	Payload map[string]interface{} `json:"payload"`
}
