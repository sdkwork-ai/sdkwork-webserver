package types


type AuditLogPage struct {
	Items []AuditLogResponse `json:"items"`
	Total string `json:"total"`
}
