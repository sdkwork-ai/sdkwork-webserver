package types


type PlatformTargetResponse struct {
	Id string `json:"id"`
	AppId string `json:"appId"`
	TargetKey string `json:"targetKey"`
	Platform Platform `json:"platform"`
	TechStack TechStack `json:"techStack"`
	Architectures []string `json:"architectures"`
	BundleId string `json:"bundleId"`
	PackageName string `json:"packageName"`
	AppIdValue string `json:"appIdValue"`
	BundleName string `json:"bundleName"`
	TargetStatus string `json:"targetStatus"`
	CreatedAt string `json:"createdAt"`
	UpdatedAt string `json:"updatedAt"`
}
