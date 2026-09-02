export interface NginxStatusResponse {
  running?: boolean;
  version?: string;
  pid?: number;
  activeConnections?: number;
  configPath?: string;
  uptime?: string;
}
