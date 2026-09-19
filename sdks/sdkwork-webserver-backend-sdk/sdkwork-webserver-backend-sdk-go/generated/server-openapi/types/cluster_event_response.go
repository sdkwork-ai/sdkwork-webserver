package types


type ClusterEventResponse struct {
	Id string `json:"id"`
	ClusterId string `json:"clusterId"`
	HostId string `json:"hostId"`
	InstanceId string `json:"instanceId"`
	EventType string `json:"eventType"`
	Severity string `json:"severity"`
	Message string `json:"message"`
	Detail map[string]interface{} `json:"detail"`
	OccurredAt string `json:"occurredAt"`
	CreatedAt string `json:"createdAt"`
}
