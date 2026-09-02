package types


type EnvVariableResponse struct {
	Id string `json:"id"`
	Key string `json:"key"`
	Environment string `json:"environment"`
	IsSecret bool `json:"isSecret"`
	CreatedAt string `json:"createdAt"`
}
