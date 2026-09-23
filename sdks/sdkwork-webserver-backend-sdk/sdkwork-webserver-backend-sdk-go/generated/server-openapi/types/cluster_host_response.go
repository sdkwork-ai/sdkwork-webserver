package types


type ClusterHostResponse struct {
	Id string `json:"id"`
	ClusterId string `json:"clusterId"`
	Name string `json:"name"`
	Hostname string `json:"hostname"`
	MachineCode string `json:"machineCode"`
	OsName string `json:"osName"`
	OsVersion string `json:"osVersion"`
	KernelVersion string `json:"kernelVersion"`
	Arch string `json:"arch"`
	CpuModel string `json:"cpuModel"`
	CpuCores int `json:"cpuCores"`
	MemoryTotalMb Int64String `json:"memoryTotalMb"`
	RemoteIp string `json:"remoteIp"`
	LocalIps []string `json:"localIps"`
	MacAddresses []string `json:"macAddresses"`
	DaemonVersion string `json:"daemonVersion"`
	Status int `json:"status"`
	LastHeartbeatAt string `json:"lastHeartbeatAt"`
	InstanceCount Int64String `json:"instanceCount"`
	JoinMode string `json:"joinMode"`
	TunnelRouteDomain string `json:"tunnelRouteDomain"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
