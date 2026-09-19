package types


type UpdateClusterInstanceRequest struct {
	Name string `json:"name"`
	Status int `json:"status"`
	PublicEndpoint string `json:"publicEndpoint"`
}
