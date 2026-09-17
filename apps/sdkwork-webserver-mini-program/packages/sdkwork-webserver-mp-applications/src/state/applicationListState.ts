import { registerWebserverMpSensitiveStateClearer } from "@sdkwork/webserver-mp-core/session";

import type { WebserverMpApplicationRow } from "../pages/applicationListPageModel";

/**
 * Package-local state slice for the applications list.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §4/§7 require sensitive state to clear
 * on logout and account/tenant switch. The slice registers itself with the core's
 * clearing registry, so a logout drops tenant-scoped rows without every screen
 * having to remember to.
 */
export interface WebserverMpApplicationListStateSlice {
  readonly page: number;
  readonly items: readonly WebserverMpApplicationRow[];
  readonly hasMore: boolean;
  readonly errorMessage: string;
}

export const initialWebserverMpApplicationListState: WebserverMpApplicationListStateSlice = {
  page: 1,
  items: [],
  hasMore: false,
  errorMessage: "",
};

export function clearWebserverMpApplicationListState(): WebserverMpApplicationListStateSlice {
  return { ...initialWebserverMpApplicationListState, items: [] };
}

export function registerWebserverMpApplicationListStateClearing(
  onClear: () => void,
): () => void {
  return registerWebserverMpSensitiveStateClearer(onClear);
}
