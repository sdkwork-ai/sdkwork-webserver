package types


type UpdateEnvVariableRequest struct {
	Value string `json:"value"`
	IsSecret bool `json:"isSecret"`
}
