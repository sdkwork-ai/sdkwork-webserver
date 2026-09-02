package types


type UpdateDomainApplicationBindingRequest struct {
	ApplicationId string `json:"applicationId"`
	IsPrimary bool `json:"isPrimary"`
}
