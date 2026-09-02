package types

type BasePlusVO struct {
    Id        interface{} `json:"id"`
    CreatedAt string      `json:"createdAt"`
    UpdatedAt string      `json:"updatedAt"`
    CreatedBy string      `json:"createdBy"`
    UpdatedBy string      `json:"updatedBy"`
}

type QueryListForm struct {
    Q           string      `json:"q"`
    Status       interface{} `json:"status"`
    StartTime    string      `json:"startTime"`
    EndTime      string      `json:"endTime"`
    OrderBy      string      `json:"orderBy"`
    OrderDirection string    `json:"orderDirection"`
}

type Page[T any] struct {
    Content     []T   `json:"content"`
    Total       int   `json:"total"`
    Page        int   `json:"page"`
    PageSize    int   `json:"pageSize"`
    TotalPages  int   `json:"totalPages"`
    HasMore     bool  `json:"hasMore"`
}
