export interface NginxConfigResponse {
  id?: string;
  configType?: number;
  configName?: string;
  configContent?: string;
  configHash?: string;
  isActive?: boolean;
  versionNo?: number;
  deployedAt?: string;
  status?: number;
  createdAt?: string;
  updatedAt?: string;
}
