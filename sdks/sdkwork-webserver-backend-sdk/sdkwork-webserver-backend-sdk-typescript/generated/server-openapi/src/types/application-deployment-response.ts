export interface ApplicationDeploymentResponse {
  id: string;
  siteId: string;
  sourceVersionId?: string;
  status: number;
  deployType: number;
  environment: 'development' | 'test' | 'staging' | 'production';
  versionTag?: string;
  commitHash?: string;
  sourceRef?: string;
  /** Immutable successful deployment selected as this restore command's source. */
  rollbackFromDeploymentId?: string;
  artifactDriveUri?: string;
  artifactSize?: string;
  artifactHash?: string;
  startedAt?: string;
  completedAt?: string;
  durationMs?: string;
  createdAt: string;
}
