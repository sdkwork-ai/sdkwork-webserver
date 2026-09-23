export interface ProbeClusterInstanceRequest {
  /** Probe path; defaults to `/`. */
  path?: string;
  /** Probe timeout, clamped to 100..=10000 ms. */
  timeoutMs?: number;
}
