export interface ClusterInstanceDescriptor {
  name?: string;
  role: 'GATEWAY' | 'MANAGEMENT' | 'DATA_PLANE' | 'WORKER' | 'OTHER';
  environment: 'development' | 'test' | 'staging' | 'production';
  processPid: number;
  processStartedAt: string;
  bindHost?: string;
  bindPort?: number;
  /** Advertised endpoint peers use to reach this instance. */
  publicEndpoint?: string;
  buildVersion?: string;
}
