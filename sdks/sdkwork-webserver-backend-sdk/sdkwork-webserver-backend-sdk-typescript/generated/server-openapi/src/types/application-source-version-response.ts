import type { ApplicationSourceVersionConfigSnapshot } from './application-source-version-config-snapshot';

export interface ApplicationSourceVersionResponse {
  id: string;
  siteId: string;
  versionTag: string;
  sourceType: 'ARCHIVE' | 'DIRECTORY' | 'GIT';
  sourceRef?: string;
  commitHash?: string;
  artifactDriveUri: string;
  artifactSize: string;
  artifactHash: string;
  configSnapshot: ApplicationSourceVersionConfigSnapshot;
  status: 0 | 1 | 2 | 3;
  retained: boolean;
  createdAt: string;
}
