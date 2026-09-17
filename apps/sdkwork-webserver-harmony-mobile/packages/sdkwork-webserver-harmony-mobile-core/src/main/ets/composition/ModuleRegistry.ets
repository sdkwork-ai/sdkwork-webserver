import {
  webserverHarmonyRouteContributions,
  type WebserverHarmonyRouteContribution,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-applications';

/**
 * Module registry.
 *
 * The root bootstrap reads this registry instead of importing capability packages
 * directly, so adding a capability is a registry change rather than a bootstrap
 * rewrite (`APP_COMPOSITION_SPEC.md` §3).
 */
export interface WebserverHarmonyModuleRegistration {
  readonly id: string;
  readonly routeIdPrefix: string;
}

export function listWebserverHarmonyModules(): WebserverHarmonyModuleRegistration[] {
  return [
    { id: 'applications', routeIdPrefix: 'app.webserver.applications.' },
  ];
}

export function listWebserverHarmonyModuleRoutes(): WebserverHarmonyRouteContribution[] {
  return webserverHarmonyRouteContributions;
}

export function countWebserverHarmonyModuleRoutes(): number {
  return webserverHarmonyRouteContributions.length;
}
