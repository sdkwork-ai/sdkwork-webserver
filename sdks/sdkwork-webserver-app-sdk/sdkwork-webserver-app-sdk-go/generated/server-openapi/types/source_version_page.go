package types


type SourceVersionPage struct {
	Items []SourceVersionResponse `json:"items"`
	Total string `json:"total"`
}
