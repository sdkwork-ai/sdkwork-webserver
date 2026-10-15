package types


type TunnelMachineRouteDeclarationResponse struct {
	RouteId string `json:"routeId"`
	Active bool `json:"active"`
	PublicUrl string `json:"publicUrl"`
	Detail string `json:"detail"`
}
