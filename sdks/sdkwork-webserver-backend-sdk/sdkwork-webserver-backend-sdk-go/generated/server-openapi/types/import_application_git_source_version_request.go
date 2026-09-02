package types


type ImportApplicationGitSourceVersionRequest struct {
	VersionTag string `json:"versionTag"`
	RepositoryUrl string `json:"repositoryUrl"`
	GitRef string `json:"gitRef"`
}
