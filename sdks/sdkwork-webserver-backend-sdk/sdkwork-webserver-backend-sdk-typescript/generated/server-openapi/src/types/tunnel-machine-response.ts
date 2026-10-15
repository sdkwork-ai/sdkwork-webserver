import type { TunnelMachineRouteResponse } from './tunnel-machine-route-response';

export interface TunnelMachineResponse {
  deviceId: string;
  deviceName: string;
  platform: string;
  online: boolean;
  enabled: boolean;
  sessionId?: string;
  firstSeenAt: string;
  lastSeenAt: string;
  connectedAt?: string;
  disconnectedAt?: string;
  remoteAddr?: string;
  bytesIn: string;
  bytesOut: string;
  sessionsTotal: string;
  routeCount: string;
  healthyRouteCount: string;
  unhealthyRouteIds?: string[];
  routes?: TunnelMachineRouteResponse[];
}
