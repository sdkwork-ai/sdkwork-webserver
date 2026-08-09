import type { SourceVersionConfigSnapshot } from './source-version-config-snapshot';

export interface SourceVersionResponse {
  id: string;
  applicationId: string;
  versionTag: string;
  sourceType: 'ARCHIVE' | 'DIRECTORY' | 'GIT';
  sourceRef?: string;
  commitHash?: string;
  artifactDriveUri: string;
  artifactSize: string;
  artifactHash: string;
  configSnapshot: SourceVersionConfigSnapshot;
  status: 0 | 1 | 2 | 3;
  retained: boolean;
  createdAt: string;
}
