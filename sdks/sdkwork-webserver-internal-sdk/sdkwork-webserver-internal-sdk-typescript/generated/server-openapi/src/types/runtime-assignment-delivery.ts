import type { RuntimeAssignment } from './runtime-assignment';
import type { RuntimeObservationState } from './runtime-observation-state';
import type { WebsiteRuntimeSetSnapshot } from './website-runtime-set-snapshot';

export interface RuntimeAssignmentDelivery {
  unchanged: boolean;
  assignment: RuntimeAssignment;
  latestObservationState?: RuntimeObservationState | null;
  runtimeSet?: WebsiteRuntimeSetSnapshot | null;
}
