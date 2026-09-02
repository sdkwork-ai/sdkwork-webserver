package types


type PageInfo struct {
	Mode string `json:"mode"`
	Page int `json:"page"`
	PageSize int `json:"pageSize"`
	TotalItems string `json:"totalItems"`
	TotalPages int `json:"totalPages"`
	NextCursor string `json:"nextCursor"`
	HasMore bool `json:"hasMore"`
}
