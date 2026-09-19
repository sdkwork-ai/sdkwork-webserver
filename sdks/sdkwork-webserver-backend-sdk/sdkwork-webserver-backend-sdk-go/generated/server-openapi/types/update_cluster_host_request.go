package types


type UpdateClusterHostRequest struct {
	Name string `json:"name"`
	ClusterId string `json:"clusterId"`
}
