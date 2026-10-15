package types


type TunnelMachineResponse struct {
	DeviceId string `json:"deviceId"`
	DeviceName string `json:"deviceName"`
	Platform string `json:"platform"`
	Online bool `json:"online"`
	Enabled bool `json:"enabled"`
	SessionId string `json:"sessionId"`
	FirstSeenAt string `json:"firstSeenAt"`
	LastSeenAt string `json:"lastSeenAt"`
	ConnectedAt string `json:"connectedAt"`
	DisconnectedAt string `json:"disconnectedAt"`
	RemoteAddr string `json:"remoteAddr"`
	BytesIn string `json:"bytesIn"`
	BytesOut string `json:"bytesOut"`
	SessionsTotal string `json:"sessionsTotal"`
	RouteCount string `json:"routeCount"`
	HealthyRouteCount string `json:"healthyRouteCount"`
	UnhealthyRouteIds []string `json:"unhealthyRouteIds"`
	Routes []TunnelMachineRouteResponse `json:"routes"`
}
