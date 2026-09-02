package types


type ServerEntry struct {
	Name string `json:"name"`
	Kind string `json:"kind"`
	Path string `json:"path"`
	Size string `json:"size"`
	ProjectType string `json:"projectType"`
	IsProjectRoot bool `json:"isProjectRoot"`
}
