package types


type AgentNginxConfigBundle struct {
	ConfigId string `json:"configId"`
	Domain string `json:"domain"`
	ConfigContent string `json:"configContent"`
	Fingerprint string `json:"fingerprint"`
	Version string `json:"version"`
}
