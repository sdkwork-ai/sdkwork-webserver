package types


type NginxConfigPage struct {
	Items []NginxConfigResponse `json:"items"`
	Total string `json:"total"`
}
