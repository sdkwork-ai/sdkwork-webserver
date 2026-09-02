import type { WebsiteRuntimeSetSnapshotOpaqueId } from './website-runtime-set-snapshot-opaque-id';
import type { WebsiteRuntimeSetSnapshotSha256 } from './website-runtime-set-snapshot-sha256';
import type { WebsiteRuntimeSetSnapshotTimestamp } from './website-runtime-set-snapshot-timestamp';

export interface WebsiteRuntimeSetSnapshot {
  schemaVersion: 'sdkwork.website-runtime-set.v1';
  kind: 'sdkwork.website-runtime-set.snapshot';
  snapshotUuid: WebsiteRuntimeSetSnapshotOpaqueId;
  nodeUuid: WebsiteRuntimeSetSnapshotOpaqueId;
  environment: 'development' | 'test' | 'staging' | 'production';
  generation: number;
  generatedAt: WebsiteRuntimeSetSnapshotTimestamp;
  compilerVersion: string;
  snapshotSha256: WebsiteRuntimeSetSnapshotSha256;
  maximumSites: number;
  descriptors: Record<string, unknown>[];
}
