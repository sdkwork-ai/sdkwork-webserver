package types


type ServerFilesNode struct {
	Id string `json:"id"`
	Name string `json:"name"`
	Host string `json:"host"`
	SshPort int `json:"sshPort"`
	Status string `json:"status"`
	FilesystemRoot string `json:"filesystemRoot"`
	Region string `json:"region"`
}
