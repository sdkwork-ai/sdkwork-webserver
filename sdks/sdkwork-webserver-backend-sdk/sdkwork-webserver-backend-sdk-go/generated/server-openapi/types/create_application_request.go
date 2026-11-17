package types

// Create input for a carrier row.

There is no `appKind` member: a carrier row's identity is
`deployApplicationId`, and its application kind is a server-owned default
(`STATIC_WEB`, which derives `siteType = 1` and the site carrier
application type `WEB`). The member was removed rather than made optional
so no caller can re-declare a second application identity on the carrier
row (`MIG-2026-0071` deletion-list item 6).

type CreateApplicationRequest struct {
	Name string `json:"name"`
	Slug string `json:"slug"`
	Description string `json:"description"`
	DeployApplicationId string `json:"deployApplicationId"`
	RuntimeConfig map[string]interface{} `json:"runtimeConfig"`
	StoreListing ApplicationStoreListing `json:"storeListing"`
}
