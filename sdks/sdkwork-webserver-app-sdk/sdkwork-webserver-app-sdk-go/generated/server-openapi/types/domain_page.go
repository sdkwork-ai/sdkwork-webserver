package types


type DomainPage struct {
	Items []DomainResponse `json:"items"`
	Total string `json:"total"`
}
