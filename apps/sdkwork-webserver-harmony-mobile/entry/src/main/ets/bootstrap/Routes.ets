import {
  webserverHarmonyRouteContributions,
  type WebserverHarmonyRouteContribution,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-applications';

/**
 * Route/page registry assembly.
 *
 * Route ids follow `<surface>.<domain>.<capability>.<screen>` and stay aligned
 * with the PC, H5, and mini program roots
 * (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §7). Physical Harmony page paths
 * may differ from the other platforms while the route ids stay identical.
 */

export function createWebserverHarmonyRoutes(): WebserverHarmonyRouteContribution[] {
  return webserverHarmonyRouteContributions;
}

export function getWebserverHarmonyRoutes(): WebserverHarmonyRouteContribution[] {
  return webserverHarmonyRouteContributions;
}
