package types


type ImportGitSourceVersionRequest struct {
	VersionTag string `json:"versionTag"`
	RepositoryUrl string `json:"repositoryUrl"`
	GitRef string `json:"gitRef"`
}
