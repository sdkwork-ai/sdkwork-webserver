package types


type ServerPage struct {
	Items []ServerResponse `json:"items"`
	Total string `json:"total"`
}
