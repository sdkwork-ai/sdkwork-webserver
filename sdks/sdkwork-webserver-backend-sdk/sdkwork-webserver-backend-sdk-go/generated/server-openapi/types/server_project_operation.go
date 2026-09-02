package types


type ServerProjectOperation struct {
	Id string `json:"id"`
	Kind string `json:"kind"`
	Label string `json:"label"`
	Permission string `json:"permission"`
	Description string `json:"description"`
	Dangerous bool `json:"dangerous"`
}
