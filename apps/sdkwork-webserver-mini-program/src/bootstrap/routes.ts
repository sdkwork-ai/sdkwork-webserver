/**
 * Root composition of the route contributions.
 *
 * The root is the only place that knows every capability, so it is the only place
 * that assembles the full route list. `scripts/build-runtime.mjs` projects it into
 * `src/app.json`, which means the platform page list and the SDKWork route
 * metadata cannot drift apart (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5/§9).
 */
import { webserverMpApplicationsRouteContributions } from "@sdkwork/webserver-mp-applications";
import type { WebserverMpRouteContribution } from "@sdkwork/webserver-mp-shell";

export function createWebserverMiniProgramRoutes(): WebserverMpRouteContribution[] {
  return [...webserverMpApplicationsRouteContributions];
}
