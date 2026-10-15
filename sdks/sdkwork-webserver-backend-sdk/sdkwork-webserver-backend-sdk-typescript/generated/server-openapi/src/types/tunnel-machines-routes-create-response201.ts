import type { TunnelMachineRouteDeclarationResponse } from './tunnel-machine-route-declaration-response';

export interface TunnelMachinesRoutesCreateResponse201 {
  code: 0;
  data: unknown & { item: TunnelMachineRouteDeclarationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
