/**
 * Environment projection for the mini program root.
 *
 * A native mini program has exactly one active runtime profile per build; it is
 * selected at write time (`scripts/build-runtime.mjs`) and recorded in
 * `src/runtime/build-manifest.json`. Nothing here recomputes it from
 * `process.env`, because there is no environment at device run time — this module
 * only re-exports the identity the build already froze.
 */
import type {
  WebserverMpDeploymentProfile,
  WebserverMpLifecycleEnvironment,
} from "@sdkwork/webserver-mp-core/sdk";

export interface WebserverMiniProgramBuildIdentity {
  readonly deploymentProfile: WebserverMpDeploymentProfile;
  readonly environment: WebserverMpLifecycleEnvironment;
  readonly profileId: string;
  readonly runtimeTarget: "mini-program";
}

export function describeWebserverMiniProgramBuildIdentity(
  identity: WebserverMiniProgramBuildIdentity,
): string {
  return `${identity.profileId} (${identity.runtimeTarget})`;
}
