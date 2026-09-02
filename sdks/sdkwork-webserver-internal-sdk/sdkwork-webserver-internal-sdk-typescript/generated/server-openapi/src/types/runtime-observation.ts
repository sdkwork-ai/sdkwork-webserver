import type { GenerationString } from './generation-string';
import type { RuntimeEnvironment } from './runtime-environment';
import type { RuntimeObservationState } from './runtime-observation-state';
import type { Sha256 } from './sha256';

export interface RuntimeObservation {
  observationUuid: string;
  assignmentUuid: string;
  tenantId: string;
  nodeUuid: string;
  environment: RuntimeEnvironment;
  generation: GenerationString;
  snapshotUuid: string;
  snapshotSha256: Sha256;
  state: RuntimeObservationState;
  nodeVersion?: string | null;
  reasonCode?: string | null;
  detail?: string | null;
  observedAt: string;
}
