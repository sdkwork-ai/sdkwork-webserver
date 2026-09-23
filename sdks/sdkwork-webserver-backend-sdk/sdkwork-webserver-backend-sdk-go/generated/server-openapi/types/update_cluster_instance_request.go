package types


type UpdateClusterInstanceRequest struct {
	Name string `json:"name"`
	Status int `json:"status"`
	PublicEndpoint string `json:"publicEndpoint"`
	RoutingEnabled bool `json:"routingEnabled"`
	Draining bool `json:"draining"`
	ProbeUrl string `json:"probeUrl"`
	Labels map[string]string `json:"labels"`
	RoutingWeight int `json:"routingWeight"`
	MaintenanceNote string `json:"maintenanceNote"`
}
