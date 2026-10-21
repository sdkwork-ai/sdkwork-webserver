export interface NginxConfigRevisionResponse {
  versionNo?: number;
  configName?: string;
  configContent?: string;
  configHash?: string;
  createdBy?: string;
  createdAt?: string;
}
