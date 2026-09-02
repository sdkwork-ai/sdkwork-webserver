package types


type ApplicationPage struct {
	Items []ApplicationResponse `json:"items"`
	Total string `json:"total"`
	Page int `json:"page"`
	PageSize int `json:"pageSize"`
}
