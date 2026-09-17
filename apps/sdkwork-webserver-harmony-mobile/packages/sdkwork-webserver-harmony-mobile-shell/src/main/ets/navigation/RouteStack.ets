import {
  validateWebserverHarmonyRouteContributions,
  type WebserverHarmonyRouteContribution,
} from './RoutePlacement';

/**
 * Page/route stack assembly for the HarmonyOS shell.
 *
 * The shell owns ordering, duplicate detection, and permission filtering, so no
 * screen hardcodes the tab bar. This mirrors the H5 and mini program shell models
 * so the same workflow surfaces identically on every client.
 */
export interface WebserverHarmonyNavigationEntry {
  /** Route id this entry opens, so navigation and routing cannot drift apart. */
  readonly id: string;
  readonly pagePath: string;
  readonly labelKey: string;
  readonly permission: string;
  readonly order: number;
}

export type WebserverHarmonyHasPermission = (permission: string) => boolean;

export function createWebserverHarmonyRouteStack(
  routes: WebserverHarmonyRouteContribution[],
): WebserverHarmonyRouteContribution[] {
  const issues: string[] = validateWebserverHarmonyRouteContributions(routes);
  if (issues.length > 0) {
    throw new Error(`invalid harmony route contributions: ${issues.join('; ')}`);
  }
  return routes.filter((route) => route.pagePath.trim().length > 0);
}

export function createWebserverHarmonyNavigation(
  routes: WebserverHarmonyRouteContribution[],
): WebserverHarmonyNavigationEntry[] {
  const entries: WebserverHarmonyNavigationEntry[] = [];
  const seenIds: string[] = [];
  const seenPaths: string[] = [];
  for (const route of routes) {
    const contribution = route.navigation;
    if (contribution === undefined || contribution === null) {
      continue;
    }
    if (seenIds.includes(route.id)) {
      throw new Error(`duplicate harmony navigation id: ${route.id}`);
    }
    if (seenPaths.includes(route.pagePath)) {
      throw new Error(`duplicate harmony navigation path: ${route.pagePath}`);
    }
    seenIds.push(route.id);
    seenPaths.push(route.pagePath);
    entries.push({
      id: route.id,
      pagePath: route.pagePath,
      labelKey: contribution.labelKey,
      permission: contribution.permission,
      order: contribution.order,
    });
  }
  return entries.sort((left, right) => left.order - right.order);
}

export function filterWebserverHarmonyNavigation(
  entries: WebserverHarmonyNavigationEntry[],
  hasPermission: WebserverHarmonyHasPermission,
): WebserverHarmonyNavigationEntry[] {
  return entries.filter((entry) => hasPermission(entry.permission));
}

/** The route a launch should land on once permissions are known. */
export function resolveWebserverHarmonyHomePagePath(
  entries: WebserverHarmonyNavigationEntry[],
): string | undefined {
  return entries.length > 0 ? entries[0].pagePath : undefined;
}
