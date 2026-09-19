package types


type ClusterResponse struct {
	Id string `json:"id"`
	Name string `json:"name"`
	Code string `json:"code"`
	Description string `json:"description"`
	Status int `json:"status"`
	HeartbeatIntervalSeconds int `json:"heartbeatIntervalSeconds"`
	OfflineThresholdSeconds int `json:"offlineThresholdSeconds"`
	HostCount Int64String `json:"hostCount"`
	InstanceCount Int64String `json:"instanceCount"`
	OnlineInstanceCount Int64String `json:"onlineInstanceCount"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
