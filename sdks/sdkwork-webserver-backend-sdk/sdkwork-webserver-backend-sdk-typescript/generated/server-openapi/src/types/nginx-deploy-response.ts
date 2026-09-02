export interface NginxDeployResponse {
  success?: boolean;
  configId?: string;
  deployedAt?: string;
  reloadResult?: { reloaded?: boolean; message?: string; };
}
