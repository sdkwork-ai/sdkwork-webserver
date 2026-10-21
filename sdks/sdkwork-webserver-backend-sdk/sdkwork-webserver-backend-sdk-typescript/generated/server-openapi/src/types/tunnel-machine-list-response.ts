import type { PageInfo } from './page-info';
import type { TunnelMachineResponse } from './tunnel-machine-response';

export interface TunnelMachineListResponse {
  machines: TunnelMachineResponse[];
  pageInfo?: PageInfo;
}
