package types


type DomainDnsSyncResponse struct {
	RecordCount Int64String `json:"recordCount"`
	SyncedAt string `json:"syncedAt"`
	ZoneApex string `json:"zoneApex"`
	DnsProvider string `json:"dnsProvider"`
	CloudAccountId string `json:"cloudAccountId"`
}
