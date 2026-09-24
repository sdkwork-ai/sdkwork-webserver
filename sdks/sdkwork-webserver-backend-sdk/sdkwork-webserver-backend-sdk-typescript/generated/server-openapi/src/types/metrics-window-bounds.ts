export interface MetricsWindowBounds {
  window: string;
  /** Inclusive UTC day. Absent for the lifetime window, whose lower bound is wherever the figures begin rather than a day this contract could invent. */
  dateFrom?: string;
  dateTo: string;
}
