package types

// Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable.
type UpdateRootDomainRequest struct {
	DisplayName string `json:"displayName"`
	DnsProvider string `json:"dnsProvider"`
	ProviderZoneRef string `json:"providerZoneRef"`
	Status int `json:"status"`
}
