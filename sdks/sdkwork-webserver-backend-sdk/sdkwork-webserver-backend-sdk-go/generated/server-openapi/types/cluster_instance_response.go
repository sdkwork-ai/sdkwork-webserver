package types


type ClusterInstanceResponse struct {
	Id string `json:"id"`
	ClusterId string `json:"clusterId"`
	HostId string `json:"hostId"`
	HostName string `json:"hostName"`
	Name string `json:"name"`
	Role string `json:"role"`
	Environment string `json:"environment"`
	ProcessPid int `json:"processPid"`
	ProcessStartedAt string `json:"processStartedAt"`
	BindHost string `json:"bindHost"`
	BindPort int `json:"bindPort"`
	PublicEndpoint string `json:"publicEndpoint"`
	BuildVersion string `json:"buildVersion"`
	Status int `json:"status"`
	HealthState string `json:"healthState"`
	LastHeartbeatAt string `json:"lastHeartbeatAt"`
	LastOnlineAt string `json:"lastOnlineAt"`
	UptimeSeconds Int64String `json:"uptimeSeconds"`
	Metrics map[string]interface{} `json:"metrics"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
