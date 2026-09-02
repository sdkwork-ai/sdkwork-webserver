import type { GenerationString } from './generation-string';
import type { RuntimeEnvironment } from './runtime-environment';
import type { Sha256 } from './sha256';

export interface RuntimeAssignment {
  assignmentUuid: string;
  nodeUuid: string;
  environment: RuntimeEnvironment;
  generation: GenerationString;
  snapshotUuid: string;
  snapshotSha256: Sha256;
  assignedAt: string;
}
