package types


type DeploymentPage struct {
	Items []DeploymentResponse `json:"items"`
	Total string `json:"total"`
}
