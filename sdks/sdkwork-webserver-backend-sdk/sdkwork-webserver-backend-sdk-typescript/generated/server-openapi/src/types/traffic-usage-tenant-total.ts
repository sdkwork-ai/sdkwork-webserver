import type { Int64String } from './int64-string';

export interface TrafficUsageTenantTotal {
  tenantId: Int64String;
  dimension: string;
  quantity: Int64String;
  unit: string;
}
