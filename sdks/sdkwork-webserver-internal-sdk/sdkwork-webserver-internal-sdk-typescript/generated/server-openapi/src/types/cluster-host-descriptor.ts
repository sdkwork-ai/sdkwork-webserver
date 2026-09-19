import type { Int64String } from './int64-string';

export interface ClusterHostDescriptor {
  hostname: string;
  /** Stable hardware/machine fingerprint of the host. */
  machineCode: string;
  name?: string;
  osName?: string;
  osVersion?: string;
  kernelVersion?: string;
  arch?: string;
  cpuModel?: string;
  cpuCores?: number;
  memoryTotalMb?: Int64String;
  remoteIp?: string;
  localIps?: string[];
  macAddresses?: string[];
  daemonVersion?: string;
}
