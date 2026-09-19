package types


type ClusterOverviewResponse struct {
	TotalHosts Int64String `json:"totalHosts"`
	OnlineHosts Int64String `json:"onlineHosts"`
	TotalInstances Int64String `json:"totalInstances"`
	OnlineInstances Int64String `json:"onlineInstances"`
	UnhealthyInstances Int64String `json:"unhealthyInstances"`
	PendingPeerMessages Int64String `json:"pendingPeerMessages"`
	GeneratedAt string `json:"generatedAt"`
}
