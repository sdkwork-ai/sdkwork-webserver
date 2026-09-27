package types


type DnsAccountResponse struct {
	AccountId string `json:"accountId"`
	Provider string `json:"provider"`
	ZoneApex string `json:"zoneApex"`
}
