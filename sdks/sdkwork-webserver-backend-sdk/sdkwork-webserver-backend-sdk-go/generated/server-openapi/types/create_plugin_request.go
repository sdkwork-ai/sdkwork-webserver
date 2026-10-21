package types


type CreatePluginRequest struct {
	PluginKey string `json:"pluginKey"`
	DisplayName string `json:"displayName"`
	Summary string `json:"summary"`
	Version string `json:"version"`
	CategoryId string `json:"categoryId"`
	SupportedHostTools []string `json:"supportedHostTools"`
	ContributedCapabilities []string `json:"contributedCapabilities"`
	SourceKind string `json:"sourceKind"`
	GitRepository string `json:"gitRepository"`
	GitRef string `json:"gitRef"`
	ArtifactRef string `json:"artifactRef"`
	ChecksumSha256 string `json:"checksumSha256"`
	ArchiveFileName string `json:"archiveFileName"`
	Status string `json:"status"`
}
