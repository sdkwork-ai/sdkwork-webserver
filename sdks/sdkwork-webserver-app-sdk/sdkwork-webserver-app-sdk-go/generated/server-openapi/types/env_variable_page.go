package types


type EnvVariablePage struct {
	Items []EnvVariableResponse `json:"items"`
	Total string `json:"total"`
}
