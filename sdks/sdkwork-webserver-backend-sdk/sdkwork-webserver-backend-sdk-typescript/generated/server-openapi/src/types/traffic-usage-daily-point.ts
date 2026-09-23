import type { Int64String } from './int64-string';

export interface TrafficUsageDailyPoint {
  usageDate: string;
  dimension: string;
  quantity: Int64String;
}
