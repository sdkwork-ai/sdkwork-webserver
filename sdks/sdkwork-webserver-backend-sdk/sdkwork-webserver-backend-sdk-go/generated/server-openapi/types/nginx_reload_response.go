package types


type NginxReloadResponse struct {
	Success bool `json:"success"`
	Message string `json:"message"`
	Timestamp string `json:"timestamp"`
}
