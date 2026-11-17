package types


type CreateApplicationRequest struct {
	Name string `json:"name"`
	Slug string `json:"slug"`
	Description string `json:"description"`
	AppKind AppKind `json:"appKind"`
	DeployApplicationId string `json:"deployApplicationId"`
	RuntimeConfig map[string]interface{} `json:"runtimeConfig"`
	StoreListing ApplicationStoreListing `json:"storeListing"`
}
