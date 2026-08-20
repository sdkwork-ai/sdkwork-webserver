package types


type CreateApplicationRequest struct {
	Name string `json:"name"`
	Slug string `json:"slug"`
	Description string `json:"description"`
	AppKind AppKind `json:"appKind"`
	RuntimeConfig map[string]interface{} `json:"runtimeConfig"`
	StoreListing ApplicationStoreListing `json:"storeListing"`
}
