package types


type ServerDirectoryListing struct {
	NodeId string `json:"nodeId"`
	Path string `json:"path"`
	ParentPath string `json:"parentPath"`
	Entries []ServerEntry `json:"entries"`
}
