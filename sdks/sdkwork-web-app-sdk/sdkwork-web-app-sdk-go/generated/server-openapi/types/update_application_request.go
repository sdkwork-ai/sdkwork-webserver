package types


type UpdateApplicationRequest struct {
	Name string `json:"name"`
	Description string `json:"description"`
	RuntimeConfig map[string]interface{} `json:"runtimeConfig"`
	StoreListing ApplicationStoreListing `json:"storeListing"`
}
