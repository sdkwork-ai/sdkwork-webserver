import type { TunnelMachineResponse } from './tunnel-machine-response';

export interface TunnelMachinesEnabledUpdateResponse {
  code: 0;
  data: unknown & { item: TunnelMachineResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
