import type { TunnelMachineListResponse } from './tunnel-machine-list-response';

export interface TunnelMachinesListResponse {
  code: 0;
  data: unknown & { item: TunnelMachineListResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
