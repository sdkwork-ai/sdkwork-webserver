package types


type ProbeClusterInstanceRequest struct {
	Path string `json:"path"`
	TimeoutMs int `json:"timeoutMs"`
}
