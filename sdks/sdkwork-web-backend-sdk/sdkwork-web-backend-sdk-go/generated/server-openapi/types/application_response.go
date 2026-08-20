package types


type ApplicationResponse struct {
	Id string `json:"id"`
	Name string `json:"name"`
	Slug string `json:"slug"`
	Description string `json:"description"`
	AppKind AppKind `json:"appKind"`
	SiteType int `json:"siteType"`
	Status int `json:"status"`
	RuntimeConfig map[string]interface{} `json:"runtimeConfig"`
	StoreListing ApplicationStoreListing `json:"storeListing"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
