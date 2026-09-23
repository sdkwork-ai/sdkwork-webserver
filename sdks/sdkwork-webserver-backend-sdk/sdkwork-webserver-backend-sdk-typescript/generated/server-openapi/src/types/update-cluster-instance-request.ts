export interface UpdateClusterInstanceRequest {
  name?: string;
  status?: number;
  publicEndpoint?: string;
  /** Cordon switch: `false` removes the instance from the routing pool while it keeps serving. */
  routingEnabled?: boolean;
  /** Graceful drain start/clear. Starting a drain also cordons routing. */
  draining?: boolean;
  /** Active-probe target override. */
  probeUrl?: string;
  /** Operator labels; replaces the whole map when present. */
  labels?: Record<string, string>;
  /** Per-instance load balancing weight override. */
  routingWeight?: number;
  /** Operator maintenance reason/context. */
  maintenanceNote?: string;
}
