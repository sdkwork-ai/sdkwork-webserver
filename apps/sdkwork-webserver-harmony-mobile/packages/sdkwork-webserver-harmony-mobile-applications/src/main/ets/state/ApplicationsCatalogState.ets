import {
  registerWebserverHarmonySensitiveStateClearer,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

import type { WebserverHarmonyApplicationRow } from '../models/ApplicationModels';

/**
 * Package-local state slice for the applications catalog.
 *
 * `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §7 requires sensitive state to clear
 * on logout and on account/tenant switch. The slice registers itself with the
 * core's clearing registry, so a logout drops tenant-scoped rows without every
 * screen having to remember to.
 */
export interface WebserverHarmonyApplicationsCatalogStateSlice {
  readonly page: number;
  readonly rows: WebserverHarmonyApplicationRow[];
  readonly hasMore: boolean;
  readonly errorMessage: string;
}

export function createInitialWebserverHarmonyApplicationsCatalogState(): WebserverHarmonyApplicationsCatalogStateSlice {
  return { page: 1, rows: [], hasMore: false, errorMessage: '' };
}

export function registerWebserverHarmonyApplicationsCatalogStateClearing(
  onClear: () => void,
): void {
  registerWebserverHarmonySensitiveStateClearer(onClear);
}
