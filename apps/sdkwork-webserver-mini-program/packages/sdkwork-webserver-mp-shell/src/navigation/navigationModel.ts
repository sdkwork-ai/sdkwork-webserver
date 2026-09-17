import type { WebserverMpRouteContribution } from "./routePlacement";

/**
 * Ordered navigation model the root page renders.
 *
 * Capability packages contribute entries; the shell owns ordering, duplicate
 * detection, and permission filtering, so no screen hardcodes the tab bar. This
 * mirrors the H5 root's shell model so the same workflow surfaces identically on
 * both clients (`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`).
 */
export interface WebserverMpNavigationEntry {
  /** Route id this entry opens, so navigation and routing cannot drift apart. */
  readonly id: string;
  readonly path: string;
  readonly labelKey: string;
  readonly permission: string;
  readonly order: number;
}

export type WebserverMpHasPermission = (permission: string) => boolean;

export function createWebserverMpNavigation(
  contributions: readonly WebserverMpRouteContribution[],
): WebserverMpNavigationEntry[] {
  const entries: WebserverMpNavigationEntry[] = [];
  const seenIds = new Set<string>();
  const seenPaths = new Set<string>();
  for (const route of contributions) {
    if (!route.navigation) continue;
    if (seenIds.has(route.id)) {
      throw new Error(`duplicate mini program navigation id: ${route.id}`);
    }
    if (seenPaths.has(route.miniProgram.pagePath)) {
      throw new Error(`duplicate mini program navigation path: ${route.miniProgram.pagePath}`);
    }
    seenIds.add(route.id);
    seenPaths.add(route.miniProgram.pagePath);
    entries.push({
      id: route.id,
      path: `/${route.miniProgram.pagePath}`,
      labelKey: route.navigation.labelKey,
      permission: route.navigation.permission,
      order: route.navigation.order,
    });
  }
  return entries.sort((left, right) => left.order - right.order);
}

export function filterWebserverMpNavigation(
  entries: readonly WebserverMpNavigationEntry[],
  hasPermission: WebserverMpHasPermission,
): WebserverMpNavigationEntry[] {
  return entries.filter((entry) => hasPermission(entry.permission));
}

/** The route a launch should land on once permissions are known. */
export function resolveWebserverMpHomePath(
  entries: readonly WebserverMpNavigationEntry[],
): string | undefined {
  return entries[0]?.path;
}
