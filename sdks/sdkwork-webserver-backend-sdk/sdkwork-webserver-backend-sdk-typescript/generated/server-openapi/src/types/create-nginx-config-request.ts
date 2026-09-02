export interface CreateNginxConfigRequest {
  configType: 1 | 2 | 3 | 4;
  configName: string;
  configContent: string;
  siteId: string;
}
