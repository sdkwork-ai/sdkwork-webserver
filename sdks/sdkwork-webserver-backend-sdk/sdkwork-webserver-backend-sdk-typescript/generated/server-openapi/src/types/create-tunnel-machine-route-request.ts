export interface CreateTunnelMachineRouteRequest {
  name: string;
  protocol: 'http' | 'tcp' | 'udp' | 'stcp' | 'sudp';
  domain?: string;
  port?: number;
  allowPublic?: boolean;
  allowedIps?: string[];
  auth?: 'none' | 'allowed_ips' | 'bearer_token';
  visitorTokens?: string[];
  locations?: string[];
  bandwidthLimitBytesPerSecond?: string;
  proxyProtocol?: 'v1' | 'v2';
  loadBalancerGroup?: string;
}
