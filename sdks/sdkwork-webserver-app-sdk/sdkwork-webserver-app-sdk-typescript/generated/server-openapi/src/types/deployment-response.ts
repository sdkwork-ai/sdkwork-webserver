export interface DeploymentResponse {
  id: string;
  applicationId: string;
  deployType: number;
  sourceVersionId?: string;
  versionTag?: string;
  commitHash?: string;
  sourceRef?: string;
  /** 此还原命令所引用的不可变历史成功版本 ID。 */
  rollbackFromDeploymentId?: string;
  environment: string;
  artifactDriveUri?: string;
  artifactSize?: string;
  artifactHash?: string;
  status: number;
  startedAt?: string;
  completedAt?: string;
  /** Deployment duration in milliseconds as a string to avoid JavaScript precision loss. */
  durationMs?: string;
  createdAt: string;
}
