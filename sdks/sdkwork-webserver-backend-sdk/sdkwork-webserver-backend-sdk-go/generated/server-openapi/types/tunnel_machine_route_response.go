package types


type TunnelMachineRouteResponse struct {
	RouteId string `json:"routeId"`
	Name string `json:"name"`
	Protocol string `json:"protocol"`
	Domain string `json:"domain"`
	Port int `json:"port"`
	SecretName string `json:"secretName"`
	Locations []string `json:"locations"`
	LoadBalancerGroup string `json:"loadBalancerGroup"`
	BandwidthLimitBytesPerSecond string `json:"bandwidthLimitBytesPerSecond"`
	ProxyProtocol string `json:"proxyProtocol"`
	AllowPublic bool `json:"allowPublic"`
	Auth string `json:"auth"`
	AllowedIps []string `json:"allowedIps"`
	Healthy bool `json:"healthy"`
}
