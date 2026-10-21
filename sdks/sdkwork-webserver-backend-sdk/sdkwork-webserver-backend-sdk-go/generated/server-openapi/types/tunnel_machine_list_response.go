package types


type TunnelMachineListResponse struct {
	Machines []TunnelMachineResponse `json:"machines"`
	PageInfo PageInfo `json:"pageInfo"`
}
