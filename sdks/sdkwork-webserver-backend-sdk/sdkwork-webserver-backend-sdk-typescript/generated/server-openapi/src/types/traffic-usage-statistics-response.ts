import type { TrafficUsageAppTotal } from './traffic-usage-app-total';
import type { TrafficUsageDailyPoint } from './traffic-usage-daily-point';
import type { TrafficUsageTenantTotal } from './traffic-usage-tenant-total';
import type { TrafficUsageTotal } from './traffic-usage-total';

/** Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently. */
export interface TrafficUsageStatisticsResponse {
  dateFrom: string;
  dateTo: string;
  /** Whether the figures cover every tenant rather than the caller's own. Reported so a surface cannot render a platform-wide number as if it were the caller's own, or the reverse. */
  platformScope: boolean;
  totals: TrafficUsageTotal[];
  daily: TrafficUsageDailyPoint[];
  apps: TrafficUsageAppTotal[];
  /** Per-tenant breakdown. Empty for a tenant-scoped read, where the answer would be the caller's own totals repeated once per dimension. */
  tenants: TrafficUsageTenantTotal[];
}
