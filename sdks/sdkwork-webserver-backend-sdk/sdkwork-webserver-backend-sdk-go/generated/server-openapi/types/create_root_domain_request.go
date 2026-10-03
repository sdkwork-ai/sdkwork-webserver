package types


type CreateRootDomainRequest struct {
	Hostname string `json:"hostname"`
	CloudAccountId string `json:"cloudAccountId"`
}
