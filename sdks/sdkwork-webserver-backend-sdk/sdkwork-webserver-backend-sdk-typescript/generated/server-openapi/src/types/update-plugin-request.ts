export interface UpdatePluginRequest {
  displayName?: string;
  summary?: string;
  version?: string;
  categoryId?: string;
  supportedHostTools?: string[];
  contributedCapabilities?: string[];
  gitRepository?: string;
  gitRef?: string;
  artifactRef?: string;
  checksumSha256?: string | null;
  archiveFileName?: string | null;
  status?: 'draft' | 'active';
}
