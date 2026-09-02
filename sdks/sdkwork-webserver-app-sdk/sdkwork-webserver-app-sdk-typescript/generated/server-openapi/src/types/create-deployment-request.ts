/** Deployment source command. Git deployments (deployType 2) require an HTTPS sourceRef and may omit artifact fields. Other deployment types require artifactDriveUri, artifactSize, and artifactHash together. */
export interface CreateDeploymentRequest {
  /** Ready, retained application source version selected for this release. */
  sourceVersionId?: string;
  /** 1 for a stored package, 2 for a Git repository, 3 for CI/CD, or 4 for API delivery. */
  deployType: 1 | 2 | 3 | 4;
  versionTag?: string;
  commitHash?: string;
  /** HTTPS Git repository URL when deployType is 2. Credentials, query parameters, and fragments are forbidden. */
  sourceRef?: string;
  /** Stable Drive resource identity for package deployments. Signed delivery URLs are forbidden. */
  artifactDriveUri?: string;
  artifactSize?: string;
  /** Lowercase SHA-256 hexadecimal digest of the uploaded package. */
  artifactHash?: string;
  environment?: 'development' | 'test' | 'staging' | 'production';
}
