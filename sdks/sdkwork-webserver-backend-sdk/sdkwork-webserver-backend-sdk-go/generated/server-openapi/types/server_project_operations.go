package types


type ServerProjectOperations struct {
	NodeId string `json:"nodeId"`
	Path string `json:"path"`
	ProjectType string `json:"projectType"`
	Operations []ServerProjectOperation `json:"operations"`
}
