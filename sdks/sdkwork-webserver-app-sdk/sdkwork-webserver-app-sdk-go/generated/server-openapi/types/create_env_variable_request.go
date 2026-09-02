package types


type CreateEnvVariableRequest struct {
	Key string `json:"key"`
	Value string `json:"value"`
	Environment string `json:"environment"`
	IsSecret bool `json:"isSecret"`
}
