export interface TunnelMachineRouteResponse {
  routeId: string;
  name: string;
  protocol: 'http' | 'tcp' | 'udp' | 'stcp' | 'sudp';
  domain?: string;
  port?: number;
  secretName?: string;
  locations?: string[];
  loadBalancerGroup?: string;
  bandwidthLimitBytesPerSecond?: string;
  proxyProtocol?: 'v1' | 'v2';
  allowPublic: boolean;
  auth: 'none' | 'allowed_ips' | 'bearer_token';
  allowedIps?: string[];
  healthy: boolean;
}
