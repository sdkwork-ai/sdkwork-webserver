export interface PluginResponse {
  id: string;
  pluginKey: string;
  displayName: string;
  summary: string;
  version: string;
  categoryId: string;
  supportedHostTools: string[];
  contributedCapabilities: string[];
  sourceKind: 'git' | 'archive';
  gitRepository?: string | null;
  gitRef?: string | null;
  artifactRef?: string | null;
  checksumSha256?: string | null;
  archiveFileName?: string | null;
  status: 'draft' | 'active';
  createdAt: string;
  updatedAt: string;
}
