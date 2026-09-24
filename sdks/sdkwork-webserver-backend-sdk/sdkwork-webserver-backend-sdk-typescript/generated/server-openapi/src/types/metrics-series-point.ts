import type { Int64String } from './int64-string';

export interface MetricsSeriesPoint {
  /** Calendar day in UTC. */
  date: string;
  quantity: Int64String;
}
