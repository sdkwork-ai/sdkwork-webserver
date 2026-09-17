/**
 * View models for the applications capability.
 *
 * API DTOs come from the generated app SDK; this file owns view models only
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §4 — local `types/` owns view models
 * and route params).
 */
import type { DeployAppKind, DeployAppStatus } from "@sdkwork/webserver-mp-core/sdk";

export interface WebserverMpApplicationItem {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly kind: DeployAppKind;
  readonly status: DeployAppStatus;
  readonly description: string;
  /** Empty string renders as "not set" instead of a placeholder glyph. */
  readonly defaultEnvironment: string;
  readonly latestReleaseTag: string;
  readonly platformTargetCount: number;
  readonly updatedAt: string;
}

export interface WebserverMpRouteParams {
  readonly appId?: string;
}
