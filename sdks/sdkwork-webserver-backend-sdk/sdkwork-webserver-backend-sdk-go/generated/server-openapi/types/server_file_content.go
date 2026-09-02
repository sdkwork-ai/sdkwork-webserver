package types


type ServerFileContent struct {
	NodeId string `json:"nodeId"`
	Path string `json:"path"`
	Content string `json:"content"`
	Size string `json:"size"`
}
