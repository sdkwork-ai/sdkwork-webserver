import type { GenerationString } from './generation-string';
import type { RuntimeObservationState } from './runtime-observation-state';
import type { Sha256 } from './sha256';

export interface CreateRuntimeObservationRequest {
  generation: GenerationString;
  snapshotSha256: Sha256;
  state: RuntimeObservationState;
  nodeVersion?: string | null;
  reasonCode?: string | null;
  detail?: string | null;
}
