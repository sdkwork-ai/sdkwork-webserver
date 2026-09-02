package types


type AuditLogResponse struct {
	Id string `json:"id"`
	OperatorId string `json:"operatorId"`
	OperatorType string `json:"operatorType"`
	Action string `json:"action"`
	TargetType string `json:"targetType"`
	TargetId string `json:"targetId"`
	TargetUuid string `json:"targetUuid"`
	IpAddress string `json:"ipAddress"`
	Changes map[string]interface{} `json:"changes"`
	CreatedAt string `json:"createdAt"`
}
