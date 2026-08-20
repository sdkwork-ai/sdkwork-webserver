package types


type CreatePlatformTargetRequest struct {
	TargetKey string `json:"targetKey"`
	Platform Platform `json:"platform"`
	TechStack TechStack `json:"techStack"`
	Architectures []string `json:"architectures"`
	BundleId string `json:"bundleId"`
	PackageName string `json:"packageName"`
	AppId string `json:"appId"`
	BundleName string `json:"bundleName"`
	AllowedChannels []string `json:"allowedChannels"`
}
