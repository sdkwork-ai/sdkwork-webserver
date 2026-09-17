/**
 * HarmonyOS route placement metadata and projection inputs.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §8 and
 * `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §7. Route ids follow
 * `<surface>.<domain>.<capability>.<screen>` and stay aligned with the PC, H5, and
 * mini program roots; the physical Harmony page path may differ.
 *
 * Route metadata must not declare HTTP API paths, SDK methods, raw URL constants,
 * or transport details.
 */

export type WebserverHarmonyRouteAuth = 'public' | 'required';

export interface WebserverHarmonyNavigationContribution {
  readonly labelKey: string;
  readonly permission: string;
  readonly order: number;
}

export interface WebserverHarmonyRouteContribution {
  readonly id: string;
  readonly surface: 'app';
  readonly domain: string;
  readonly capability: string;
  readonly screen: string;
  /** Harmony page path, e.g. `pages/applications/ApplicationsCatalog`. */
  readonly pagePath: string;
  readonly titleKey: string;
  readonly auth: WebserverHarmonyRouteAuth;
  readonly permissionHint?: string;
  /** Navigation slot this route occupies, and its idle position in the tab bar. */
  readonly navigation?: WebserverHarmonyNavigationContribution;
}

/**
 * Validate the canonical route id shape, uniqueness, and the placement/identity
 * agreement. Returns issues rather than throwing so the contract test can report
 * every violation in one run.
 */
export function validateWebserverHarmonyRouteContributions(
  routes: WebserverHarmonyRouteContribution[],
): string[] {
  const issues: string[] = [];
  const seenIds: string[] = [];
  const seenPaths: string[] = [];
  for (const route of routes) {
    const expectedId: string =
      `${route.surface}.${route.domain}.${route.capability}.${route.screen}`;
    if (route.id !== expectedId) {
      issues.push(`route id ${route.id} must equal ${expectedId}`);
    }
    if (seenIds.includes(route.id)) {
      issues.push(`duplicate harmony route id ${route.id}`);
    }
    seenIds.push(route.id);
    if (seenPaths.includes(route.pagePath)) {
      issues.push(`duplicate harmony page path ${route.pagePath}`);
    }
    seenPaths.push(route.pagePath);
    if (route.pagePath.trim().length === 0) {
      issues.push(`route ${route.id} must declare a page path`);
    }
    if (route.navigation !== undefined && route.permissionHint === undefined) {
      issues.push(`route ${route.id} contributes navigation but declares no permissionHint`);
    }
  }
  return issues;
}
