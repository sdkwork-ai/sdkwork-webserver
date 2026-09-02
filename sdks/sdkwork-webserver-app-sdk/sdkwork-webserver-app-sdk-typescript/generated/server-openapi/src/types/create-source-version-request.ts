import type { SourceVersionConfigSnapshot } from './source-version-config-snapshot';

export interface CreateSourceVersionRequest {
  versionTag: string;
  sourceType: 'ARCHIVE' | 'DIRECTORY';
  sourceRef?: string;
  commitHash?: string;
  artifactDriveUri: string;
  artifactSize: string;
  artifactHash: string;
  configSnapshot?: SourceVersionConfigSnapshot;
}
