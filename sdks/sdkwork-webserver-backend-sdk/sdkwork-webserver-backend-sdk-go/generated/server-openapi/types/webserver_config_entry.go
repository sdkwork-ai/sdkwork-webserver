package types


type WebserverConfigEntry struct {
	Id string `json:"id"`
	Kind string `json:"kind"`
	Name string `json:"name"`
	Path string `json:"path"`
	Language string `json:"language"`
	Size string `json:"size"`
	UpdatedAt string `json:"updatedAt"`
	Writable bool `json:"writable"`
}
