export interface ImportGitSourceVersionRequest {
  versionTag: string;
  repositoryUrl: string;
  gitRef?: string;
}
