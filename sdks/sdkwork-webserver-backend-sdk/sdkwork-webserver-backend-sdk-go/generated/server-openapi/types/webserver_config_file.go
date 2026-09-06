package types


type WebserverConfigFile struct {
	Id string `json:"id"`
	Kind string `json:"kind"`
	Name string `json:"name"`
	Path string `json:"path"`
	Language string `json:"language"`
	Writable bool `json:"writable"`
	Content string `json:"content"`
	Size string `json:"size"`
	Sha256 string `json:"sha256"`
	UpdatedAt string `json:"updatedAt"`
}
