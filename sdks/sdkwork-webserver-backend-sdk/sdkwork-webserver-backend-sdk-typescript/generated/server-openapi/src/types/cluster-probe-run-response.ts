export interface ClusterProbeRunResponse {
  /** Probe reached the instance and got a healthy answer. */
  healthy: boolean;
  /** Round-trip latency of the probe. */
  latencyMs: number;
  /** Consecutive probe failures after this run; 0 when healthy. */
  failures: number;
  /** Auto-eject transition happened on this run. */
  ejected: boolean;
  /** Auto-recovery transition happened on this run. */
  recovered: boolean;
}
