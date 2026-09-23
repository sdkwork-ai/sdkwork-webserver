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
	JoinMode string `json:"joinMode"`
	QualityScore int `json:"qualityScore"`
	DesiredConfigRevision string `json:"desiredConfigRevision"`
	AppliedConfigRevision string `json:"appliedConfigRevision"`
	DesiredApplicationsRevision string `json:"desiredApplicationsRevision"`
	AppliedApplicationsRevision string `json:"appliedApplicationsRevision"`
	SyncStatus string `json:"syncStatus"`
	RoutingEnabled bool `json:"routingEnabled"`
	Draining bool `json:"draining"`
	Ejected bool `json:"ejected"`
	RestartCount int `json:"restartCount"`
	Labels map[string]string `json:"labels"`
	RoutingWeight int `json:"routingWeight"`
	MaintenanceNote string `json:"maintenanceNote"`
	ProbeFailures int `json:"probeFailures"`
	ProbeUrl string `json:"probeUrl"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
