package types


type NginxValidateResponse struct {
	Valid bool `json:"valid"`
	Errors []map[string]interface{} `json:"errors"`
}
