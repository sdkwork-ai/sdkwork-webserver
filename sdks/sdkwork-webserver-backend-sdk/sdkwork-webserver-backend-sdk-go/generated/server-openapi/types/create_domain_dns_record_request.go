package types


type CreateDomainDnsRecordRequest struct {
	RecordType string `json:"recordType"`
	Host string `json:"host"`
	RecordValue string `json:"recordValue"`
	TtlSeconds int `json:"ttlSeconds"`
	Priority int `json:"priority"`
	RecordLine string `json:"recordLine"`
}
