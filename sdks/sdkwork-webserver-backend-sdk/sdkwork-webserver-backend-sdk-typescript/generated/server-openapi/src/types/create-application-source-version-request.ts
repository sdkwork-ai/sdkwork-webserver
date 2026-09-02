import type { ApplicationSourceVersionConfigSnapshot } from './application-source-version-config-snapshot';

export interface CreateApplicationSourceVersionRequest {
  versionTag: string;
  sourceType: 'ARCHIVE' | 'DIRECTORY';
  sourceRef?: string;
  commitHash?: string;
  artifactDriveUri: string;
  artifactSize: string;
  artifactHash: string;
  configSnapshot?: ApplicationSourceVersionConfigSnapshot;
}
