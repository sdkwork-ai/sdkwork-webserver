import type { Int64String } from './int64-string';

export interface ClusterHostResponse {
  id: string;
  clusterId: string;
  name: string;
  hostname: string;
  machineCode: string;
  osName?: string;
  osVersion?: string;
  kernelVersion?: string;
  arch?: string;
  cpuModel?: string;
  cpuCores?: number;
  memoryTotalMb?: Int64String;
  remoteIp?: string;
  localIps: string[];
  macAddresses: string[];
  daemonVersion?: string;
  /** 0=offline, 1=online, 2=deploying, 3=error, 4=maintenance */
  status: number;
  lastHeartbeatAt?: string;
  instanceCount: Int64String;
  /** `LAN` = same-subnet host with shared-database or direct-API reachability; `TUNNEL` = API-only host reached through the reverse tunnel. */
  joinMode?: 'LAN' | 'TUNNEL';
  /** Tunnel route domain for `TUNNEL` hosts; absent on `LAN` hosts. */
  tunnelRouteDomain?: string;
  createdAt: string;
  updatedAt: string;
}
