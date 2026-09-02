package types


type NginxStatusResponse struct {
	Running bool `json:"running"`
	Version string `json:"version"`
	Pid int `json:"pid"`
	ActiveConnections int `json:"activeConnections"`
	ConfigPath string `json:"configPath"`
	Uptime string `json:"uptime"`
}
