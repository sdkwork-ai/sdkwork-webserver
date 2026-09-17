/**
 * View models for the applications capability.
 *
 * API DTOs come from the injected generated app SDK port; this file owns view
 * models and screen models only
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §4 — local `models/` owns view
 * models and route params).
 */

export interface WebserverHarmonyApplicationItem {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly appKind: string;
  readonly appStatus: string;
  readonly description: string;
  /** Empty string renders as "not set" instead of a placeholder glyph. */
  readonly defaultEnvironment: string;
  readonly latestReleaseTag: string;
  readonly platformTargetCount: number;
  readonly updatedAt: string;
}

/** A list row: the view model plus the labels the ArkUI list renders. */
export interface WebserverHarmonyApplicationRow extends WebserverHarmonyApplicationItem {
  readonly kindLabel: string;
  readonly statusLabel: string;
}

export interface WebserverHarmonyApplicationsRouteParams {
  readonly appId?: string;
}

/** Everything the applications catalog screen renders. */
export interface WebserverHarmonyApplicationsScreenModel {
  readonly rows: WebserverHarmonyApplicationRow[];
  readonly loading: boolean;
  /** True while an *additional* page is in flight rather than the first one. */
  readonly appending: boolean;
  readonly hasMore: boolean;
  /** Display copy for the last failure. */
  readonly errorMessage: string;
  /** Technical detail of the last failure; never treated as display copy. */
  readonly errorDetail: string;
  readonly page: number;
  readonly totalItems: number;
  readonly emptyMessage: string;
  readonly loadingMessage: string;
  readonly loadingMoreMessage: string;
  readonly retryLabel: string;
  readonly loadMoreLabel: string;
}
