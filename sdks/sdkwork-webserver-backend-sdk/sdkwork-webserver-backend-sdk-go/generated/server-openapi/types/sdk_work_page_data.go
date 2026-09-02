package types


type SdkWorkPageData struct {
	Items []map[string]interface{} `json:"items"`
	PageInfo PageInfo `json:"pageInfo"`
}
