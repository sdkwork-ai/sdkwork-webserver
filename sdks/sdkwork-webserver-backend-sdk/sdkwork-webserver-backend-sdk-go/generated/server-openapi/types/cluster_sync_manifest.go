package types


type ClusterSyncManifest struct {
	ClusterId string `json:"clusterId"`
	Kind string `json:"kind"`
	Revision string `json:"revision"`
	Sha256 string `json:"sha256"`
	Payload map[string]interface{} `json:"payload"`
	CreatedAt string `json:"createdAt"`
}
