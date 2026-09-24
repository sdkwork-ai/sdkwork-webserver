import type { Int64String } from './int64-string';

export interface MetricsWindowValue {
  window: string;
  quantity: Int64String;
  unit: string;
}
