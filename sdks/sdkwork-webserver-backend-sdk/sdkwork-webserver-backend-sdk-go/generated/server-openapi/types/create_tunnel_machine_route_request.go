package types


type CreateTunnelMachineRouteRequest struct {
	Name string `json:"name"`
	Protocol string `json:"protocol"`
	Domain string `json:"domain"`
	Port int `json:"port"`
	AllowPublic bool `json:"allowPublic"`
	AllowedIps []string `json:"allowedIps"`
	Auth string `json:"auth"`
	VisitorTokens []string `json:"visitorTokens"`
	Locations []string `json:"locations"`
	BandwidthLimitBytesPerSecond string `json:"bandwidthLimitBytesPerSecond"`
	ProxyProtocol string `json:"proxyProtocol"`
	LoadBalancerGroup string `json:"loadBalancerGroup"`
}
