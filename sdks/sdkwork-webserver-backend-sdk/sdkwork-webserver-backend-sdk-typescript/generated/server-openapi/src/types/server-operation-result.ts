export interface ServerOperationResult {
  operationId: string;
  /** Foreground runs only; absent when the run timed out. */
  exitCode?: number | null;
  timedOut?: boolean;
  stdout?: string;
  stderr?: string;
  /** True when stdout exceeded the capture cap and was truncated. */
  stdoutTruncated?: boolean;
  /** True when stderr exceeded the capture cap and was truncated. */
  stderrTruncated?: boolean;
  /** Managed runs; the recorded (or restarted) process id. */
  pid?: number;
  /** Managed runs; path of the pid-file record. */
  pidFile?: string;
  /** Managed runs; path of the append-only run log. */
  logFile?: string;
  /** Managed stops/restarts; false when no live process existed. */
  stopped?: boolean;
  /** Human-readable outcome for managed runs. */
  message?: string;
}
