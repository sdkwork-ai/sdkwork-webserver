import type { Int64String } from './int64-string';

export interface TrafficUsageTotal {
  dimension: string;
  quantity: Int64String;
  unit: string;
}
