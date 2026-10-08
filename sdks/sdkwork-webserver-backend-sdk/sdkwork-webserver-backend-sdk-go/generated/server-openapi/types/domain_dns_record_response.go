package types


type DomainDnsRecordResponse struct {
	Id string `json:"id"`
	RecordName string `json:"recordName"`
	Host string `json:"host"`
	RecordType string `json:"recordType"`
	RecordValue string `json:"recordValue"`
	TtlSeconds int `json:"ttlSeconds"`
	Priority int `json:"priority"`
	RecordLine string `json:"recordLine"`
	RecordStatus string `json:"recordStatus"`
	DomainId string `json:"domainId"`
	DnsProvider string `json:"dnsProvider"`
	CloudAccountId string `json:"cloudAccountId"`
	ProviderRecordRef string `json:"providerRecordRef"`
	SyncedAt string `json:"syncedAt"`
}
